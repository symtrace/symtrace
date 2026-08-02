//! Refactor Pattern Detection
//!
//! Analyses a set of diff operations and the underlying AST nodes to
//! recognise common refactoring patterns:
//!
//! * **extract_method** — a new function appeared while an existing function
//!   was modified, and the new function shares > 80 % subtree similarity
//!   with part of the modified function's old body.
//! * **move_method** — a function has the same `structural_hash`, appears in
//!   a different file (or different parent), and the parent context changed.
//! * **rename_variable** — a node has the same `structural_hash` (pure tree
//!   shape) but only identifier tokens differ.

use crate::node_identity;
use crate::types::{AstNode, OperationRecord, OperationType, RefactorKind, RefactorPattern};

// ── Thresholds ───────────────────────────────────────────────────────

/// Minimum subtree similarity for an extracted method to be recognised.
const EXTRACT_SUBTREE_THRESHOLD: f64 = 0.80;

// ── Significant node — lightweight descriptor for matching ───────────

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct NodeDescriptor {
    pub name: String,
    pub kind: String,
    pub structural_hash: [u8; 32],
    pub content_hash: [u8; 32],
    pub identity_hash: [u8; 32],
    /// Full AST node (needed for deep similarity comparisons)
    pub node: AstNode,
}

// ── Public API ───────────────────────────────────────────────────────

/// Detect refactoring patterns from a list of operations and optional old/new
/// ASTs for a single file.
pub fn detect_patterns(
    ops: &[OperationRecord],
    old_ast: Option<&AstNode>,
    new_ast: Option<&AstNode>,
) -> Vec<RefactorPattern> {
    let mut patterns = Vec::new();

    let old_nodes = old_ast.map(collect_function_nodes).unwrap_or_default();
    let new_nodes = new_ast.map(collect_function_nodes).unwrap_or_default();

    // ── 1. Extract-method detection ──────────────────────────────────
    patterns.extend(detect_extract_method(ops, &old_nodes, &new_nodes));

    // ── 2. Move-method detection ─────────────────────────────────────
    patterns.extend(detect_move_method(ops));

    // ── 3. Rename-variable detection ─────────────────────────────────
    patterns.extend(detect_rename_variable(ops, &old_nodes, &new_nodes));

    patterns
}

// ── Extract-method ───────────────────────────────────────────────────

fn detect_extract_method(
    ops: &[OperationRecord],
    old_nodes: &[NodeDescriptor],
    new_nodes: &[NodeDescriptor],
) -> Vec<RefactorPattern> {
    let mut patterns = Vec::new();

    // Identify inserted functions
    let inserted_names: Vec<&str> = ops
        .iter()
        .filter(|o| o.op_type == OperationType::Insert && is_function_entity(o))
        .filter_map(|o| extract_name_from_details(&o.details))
        .collect();

    // Identify modified functions
    let modified_names: Vec<&str> = ops
        .iter()
        .filter(|o| o.op_type == OperationType::Modify && is_function_entity(o))
        .filter_map(|o| extract_name_from_details(&o.details))
        .collect();

    if inserted_names.is_empty() || modified_names.is_empty() {
        return patterns;
    }

    for inserted_name in &inserted_names {
        // Find the new AST node for the inserted function
        let inserted_node = new_nodes.iter().find(|n| n.name == *inserted_name);

        for modified_name in &modified_names {
            // Find the old AST node for the modified function
            let original_node = old_nodes.iter().find(|n| n.name == *modified_name);

            if let (Some(ins), Some(orig)) = (inserted_node, original_node) {
                // Check subtree similarity between inserted function and
                // the original (pre-modification) function body.
                let sim = node_identity::structural_similarity(&ins.node, &orig.node);
                if sim > EXTRACT_SUBTREE_THRESHOLD {
                    patterns.push(RefactorPattern {
                        kind: RefactorKind::ExtractMethod,
                        description: format!(
                            "Function '{}' appears to be extracted from '{}'",
                            inserted_name, modified_name
                        ),
                        involved_entities: vec![
                            inserted_name.to_string(),
                            modified_name.to_string(),
                        ],
                        confidence: sim,
                    });
                }
            }
        }
    }

    patterns
}

// ── Move-method ──────────────────────────────────────────────────────

fn detect_move_method(ops: &[OperationRecord]) -> Vec<RefactorPattern> {
    let mut patterns = Vec::new();

    for op in ops {
        if op.op_type == OperationType::Move && is_function_entity(op) {
            let name = extract_name_from_details(&op.details).unwrap_or("unknown");
            patterns.push(RefactorPattern {
                kind: RefactorKind::MoveMethod,
                description: format!(
                    "Method '{}' moved from {} to {}",
                    name,
                    op.old_location.as_deref().unwrap_or("?"),
                    op.new_location.as_deref().unwrap_or("?"),
                ),
                involved_entities: vec![name.to_string()],
                confidence: 1.0,
            });
        }
    }

    patterns
}

// ── Rename-variable ──────────────────────────────────────────────────

fn detect_rename_variable(
    ops: &[OperationRecord],
    old_nodes: &[NodeDescriptor],
    new_nodes: &[NodeDescriptor],
) -> Vec<RefactorPattern> {
    let mut patterns = Vec::new();

    for op in ops {
        if op.op_type != OperationType::Rename {
            continue;
        }

        let old_name = extract_old_name_from_rename(&op.details);
        let new_name = extract_new_name_from_rename(&op.details);

        if let (Some(on), Some(nn)) = (old_name, new_name) {
            // Try to find matching AST nodes and verify only identifiers changed
            let old_desc = old_nodes.iter().find(|n| n.name == on);
            let new_desc = new_nodes.iter().find(|n| n.name == nn);

            let only_id_changed = match (old_desc, new_desc) {
                (Some(o), Some(n)) => node_identity::only_identifiers_changed(&o.node, &n.node),
                _ => true, // If we can't find nodes, trust the RENAME op
            };

            if only_id_changed {
                patterns.push(RefactorPattern {
                    kind: RefactorKind::RenameVariable,
                    description: format!("'{}' renamed to '{}'", on, nn),
                    involved_entities: vec![on.to_string(), nn.to_string()],
                    confidence: 1.0,
                });
            }
        }
    }

    patterns
}

// ── Helpers ──────────────────────────────────────────────────────────

fn is_function_entity(op: &OperationRecord) -> bool {
    matches!(op.entity_type, crate::types::EntityType::Function)
}

/// Extract the name between single quotes in a detail string like
/// `"function_item 'foo' inserted"`.
fn extract_name_from_details(details: &str) -> Option<&str> {
    let start = details.find('\'')?;
    let rest = &details[start + 1..];
    let end = rest.find('\'')?;
    Some(&rest[..end])
}

/// From `"function_item renamed from 'old' to 'new'"`, extract `old`.
fn extract_old_name_from_rename(details: &str) -> Option<&str> {
    let marker = "from '";
    let start = details.find(marker)? + marker.len();
    let rest = &details[start..];
    let end = rest.find('\'')?;
    Some(&rest[..end])
}

/// From `"function_item renamed from 'old' to 'new'"`, extract `new`.
fn extract_new_name_from_rename(details: &str) -> Option<&str> {
    let marker = "to '";
    let start = details.find(marker)? + marker.len();
    let rest = &details[start..];
    let end = rest.find('\'')?;
    Some(&rest[..end])
}

/// Collect function-level nodes with their full AST subtree.
fn collect_function_nodes(node: &AstNode) -> Vec<NodeDescriptor> {
    let mut result = Vec::new();
    collect_function_nodes_inner(node, &mut result);
    result
}

fn collect_function_nodes_inner(node: &AstNode, out: &mut Vec<NodeDescriptor>) {
    if is_function_kind(&node.kind) {
        let name = extract_ast_name(node);
        out.push(NodeDescriptor {
            name,
            kind: node.kind.clone(),
            structural_hash: node.structural_hash,
            content_hash: node.content_hash,
            identity_hash: node.identity_hash,
            node: node.clone(),
        });
    }
    for child in &node.children {
        collect_function_nodes_inner(child, out);
    }
}

fn is_function_kind(kind: &str) -> bool {
    matches!(
        kind,
        "function_item"
            | "function_definition"
            | "function_declaration"
            | "method_definition"
            | "method_declaration"
            | "arrow_function"
            | "closure_expression"
            | "lambda"
    )
}

/// Extract an identifier name from the AST node's children.
fn extract_ast_name(node: &AstNode) -> String {
    for child in &node.children {
        if node_identity::is_identifier_kind(&child.kind) && !child.text.is_empty() {
            return child.text.clone();
        }
    }
    for child in &node.children {
        for grandchild in &child.children {
            if node_identity::is_identifier_kind(&grandchild.kind) && !grandchild.text.is_empty() {
                return grandchild.text.clone();
            }
        }
    }
    format!("anon@L{}", node.start_row + 1)
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast_builder::parse_content;
    use crate::types::{EntityType, OperationType, ParserLimits, SupportedLanguage};
    fn parse(src: &str, lang: SupportedLanguage) -> AstNode {
        parse_content(src, lang, false, &ParserLimits::default()).expect("parse failed")
    }

    fn make_op(op_type: OperationType, entity: EntityType, details: &str) -> OperationRecord {
        OperationRecord {
            op_type,
            entity_type: entity,
            old_location: Some("L1".to_string()),
            new_location: Some("L5".to_string()),
            details: details.to_string(),
            similarity: None,
        }
    }

    #[test]
    fn detect_move_method_from_move_op() {
        let ops = vec![make_op(
            OperationType::Move,
            EntityType::Function,
            "function_item 'helper' moved",
        )];
        let patterns = detect_patterns(&ops, None, None);
        assert!(
            patterns.iter().any(|p| p.kind == RefactorKind::MoveMethod),
            "expected MoveMethod pattern, got: {patterns:?}"
        );
    }

    #[test]
    fn detect_rename_variable_from_rename_op() {
        let ops = vec![make_op(
            OperationType::Rename,
            EntityType::Function,
            "function_item renamed from 'old_name' to 'new_name'",
        )];
        let a = parse(
            "fn old_name(x: i32) -> i32 { x + 1 }",
            SupportedLanguage::Rust,
        );
        let b = parse(
            "fn new_name(x: i32) -> i32 { x + 1 }",
            SupportedLanguage::Rust,
        );
        let patterns = detect_patterns(&ops, Some(&a), Some(&b));
        assert!(
            patterns
                .iter()
                .any(|p| p.kind == RefactorKind::RenameVariable),
            "expected RenameVariable pattern, got: {patterns:?}"
        );
    }

    #[test]
    fn no_patterns_for_simple_insert() {
        let ops = vec![make_op(
            OperationType::Insert,
            EntityType::Function,
            "function_item 'new_func' inserted",
        )];
        let patterns = detect_patterns(&ops, None, None);
        assert!(
            patterns.is_empty(),
            "simple insert without modify should not produce patterns, got: {patterns:?}"
        );
    }

    #[test]
    fn extract_name_from_details_works() {
        assert_eq!(
            extract_name_from_details("function_item 'foo' inserted"),
            Some("foo")
        );
        assert_eq!(extract_name_from_details("no quotes here"), None);
    }

    #[test]
    fn rename_name_extraction() {
        let d = "function_item renamed from 'alpha' to 'beta'";
        assert_eq!(extract_old_name_from_rename(d), Some("alpha"));
        assert_eq!(extract_new_name_from_rename(d), Some("beta"));
    }
}
