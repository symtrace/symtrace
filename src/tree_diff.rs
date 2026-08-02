use std::collections::{HashMap, HashSet};

use crate::node_identity;
use crate::semantic_similarity;
use crate::types::{AstNode, EntityType, OperationRecord, OperationType};

// ── Internal node representation for diff matching ───────────────────

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct SignificantNode {
    id: u64,
    kind: String,
    name: String,
    structural_hash: [u8; 32],
    content_hash: [u8; 32],
    context_hash: [u8; 32],
    identity_hash: [u8; 32],
    start_row: usize,
    end_row: usize,
    path: Vec<String>,
    /// Full AST subtree – kept so we can run deep similarity scoring.
    ast_node: AstNode,
    /// Cached subtree node count (avoids redundant traversals).
    subtree_size: u64,
}

#[derive(Debug, Clone, Copy)]
enum MatchType {
    Moved,
    Renamed,
    Modified,
}

// ── Hash-bucket index for efficient candidate lookup ─────────────────

/// Pre-computed hash-bucket index over B-side significant nodes.
/// Built once in O(n), then enables O(1) candidate lookups per phase
/// instead of scanning all B-nodes for every A-node.
struct NodeIndex {
    /// structural_hash → indices (Phase 1 & 2: exact/structure match)
    by_structural_hash: HashMap<[u8; 32], Vec<usize>>,
    /// kind → (name → indices) (Phase 3a: same-name matching)
    by_kind_name: HashMap<String, HashMap<String, Vec<usize>>>,
    /// identity_hash → indices (Phase 3b: rename detection)
    by_identity_hash: HashMap<[u8; 32], Vec<usize>>,
    /// kind → Vec<(index, subtree_size)> (Phase 3c: similarity with size pre-filter)
    by_kind_with_size: HashMap<String, Vec<(usize, u64)>>,
}

impl NodeIndex {
    fn build(nodes: &[SignificantNode]) -> Self {
        let mut by_structural_hash: HashMap<[u8; 32], Vec<usize>> = HashMap::new();
        let mut by_kind_name: HashMap<String, HashMap<String, Vec<usize>>> = HashMap::new();
        let mut by_identity_hash: HashMap<[u8; 32], Vec<usize>> = HashMap::new();
        let mut by_kind_with_size: HashMap<String, Vec<(usize, u64)>> = HashMap::new();

        for (i, n) in nodes.iter().enumerate() {
            by_structural_hash
                .entry(n.structural_hash)
                .or_default()
                .push(i);

            if !n.name.is_empty() {
                by_kind_name
                    .entry(n.kind.clone())
                    .or_default()
                    .entry(n.name.clone())
                    .or_default()
                    .push(i);
            }

            by_identity_hash.entry(n.identity_hash).or_default().push(i);

            let size = n.subtree_size;
            by_kind_with_size
                .entry(n.kind.clone())
                .or_default()
                .push((i, size));
        }

        NodeIndex {
            by_structural_hash,
            by_kind_name,
            by_identity_hash,
            by_kind_with_size,
        }
    }
}

// ── Public API ───────────────────────────────────────────────────────

/// Compute the semantic diff between two optional ASTs.
///
/// * `(None, None)` → no operations
/// * `(None, Some)` → everything is INSERT
/// * `(Some, None)` → everything is DELETE
/// * `(Some, Some)` → structural diff
pub fn compute_diff(
    ast_a: Option<&AstNode>,
    ast_b: Option<&AstNode>,
    _logic_only: bool,
) -> Vec<OperationRecord> {
    match (ast_a, ast_b) {
        (None, None) => vec![],
        (None, Some(b)) => collect_all_as_inserts(b),
        (Some(a), None) => collect_all_as_deletes(a),
        (Some(a), Some(b)) => compute_structural_diff(a, b),
    }
}

/// Return the total number of nodes in an AST (for benchmarking metrics).
#[inline]
pub fn count_nodes(node: &AstNode) -> u64 {
    1 + node.children.iter().map(count_nodes).sum::<u64>()
}

// ── Structural diff algorithm ────────────────────────────────────────
//
// Matching priority (from the stable-node-identity spec):
//   1. exact_structure_hash + content_hash  →  identical
//   2. structure_hash match + token diff    →  rename / minor modify
//   3. composite similarity_score ≥ threshold →  modify / rename
//   4. unmatched                            →  insert / delete

fn compute_structural_diff(ast_a: &AstNode, ast_b: &AstNode) -> Vec<OperationRecord> {
    let nodes_a = collect_significant_nodes(ast_a, &[]);
    let nodes_b = collect_significant_nodes(ast_b, &[]);

    // Build hash-bucket index for B-side nodes (O(n) build, O(1) lookups)
    let index_b = NodeIndex::build(&nodes_b);

    let mut matched_a: HashSet<u64> = HashSet::with_capacity(nodes_a.len());
    let mut matched_b: HashSet<u64> = HashSet::with_capacity(nodes_b.len());
    // Store indices into nodes_a/nodes_b instead of cloning full AST subtrees
    let mut matches: Vec<(usize, usize, MatchType)> = Vec::with_capacity(nodes_a.len());

    // ── Phase 1: exact structural + content hash match ───────────────
    //    Same tree shape AND same leaf tokens → identical (or moved).
    //    Indexed lookup by structural_hash instead of scanning all B-nodes.
    for (i_a, na) in nodes_a.iter().enumerate() {
        if matched_a.contains(&na.id) {
            continue;
        }
        if let Some(candidates) = index_b.by_structural_hash.get(&na.structural_hash) {
            for &idx in candidates {
                let nb = &nodes_b[idx];
                if matched_b.contains(&nb.id) {
                    continue;
                }
                if na.content_hash == nb.content_hash {
                    matched_a.insert(na.id);
                    matched_b.insert(nb.id);
                    // Path changed → moved
                    if na.path != nb.path {
                        matches.push((i_a, idx, MatchType::Moved));
                    }
                    // else: truly identical — no operation needed
                    break;
                }
            }
        }
    }

    // ── Phase 2: structure_hash match + token diff ───────────────────
    //    Same tree shape but different content → rename or minor modify.
    //    Indexed lookup by structural_hash.
    for (i_a, na) in nodes_a.iter().enumerate() {
        if matched_a.contains(&na.id) {
            continue;
        }
        if let Some(candidates) = index_b.by_structural_hash.get(&na.structural_hash) {
            for &idx in candidates {
                let nb = &nodes_b[idx];
                if matched_b.contains(&nb.id) {
                    continue;
                }
                if na.kind == nb.kind {
                    matched_a.insert(na.id);
                    matched_b.insert(nb.id);
                    if na.name == nb.name {
                        // Same shape, same name, different tokens → modify
                        matches.push((i_a, idx, MatchType::Modified));
                    } else if node_identity::only_identifiers_changed(&na.ast_node, &nb.ast_node) {
                        // Same shape, only identifiers differ → rename
                        matches.push((i_a, idx, MatchType::Renamed));
                    } else {
                        matches.push((i_a, idx, MatchType::Modified));
                    }
                    break;
                }
            }
        }
    }

    // ── Phase 3: similarity-score matching ───────────────────────────
    //
    //    3a. Same-name matching first (MODIFY priority).
    //    Indexed lookup by (kind, name) — no full scan of B-nodes.
    for (i_a, na) in nodes_a.iter().enumerate() {
        if matched_a.contains(&na.id) {
            continue;
        }
        if na.name.is_empty() {
            continue;
        }
        if let Some(by_name) = index_b.by_kind_name.get(na.kind.as_str()) {
            if let Some(candidates) = by_name.get(na.name.as_str()) {
                for &idx in candidates {
                    let nb = &nodes_b[idx];
                    if matched_b.contains(&nb.id) {
                        continue;
                    }
                    matched_a.insert(na.id);
                    matched_b.insert(nb.id);
                    matches.push((i_a, idx, MatchType::Modified));
                    break;
                }
            }
        }
    }

    //    3b. Identity-hash matching (rename detection, ≥ RENAME_THRESHOLD).
    //    Indexed lookup by identity_hash with inline kind check.
    for (i_a, na) in nodes_a.iter().enumerate() {
        if matched_a.contains(&na.id) {
            continue;
        }
        if let Some(candidates) = index_b.by_identity_hash.get(&na.identity_hash) {
            for &idx in candidates {
                let nb = &nodes_b[idx];
                if matched_b.contains(&nb.id) {
                    continue;
                }
                if na.kind != nb.kind {
                    continue;
                }
                let sim = node_identity::composite_similarity(&na.ast_node, &nb.ast_node);
                if sim >= node_identity::RENAME_THRESHOLD {
                    matched_a.insert(na.id);
                    matched_b.insert(nb.id);
                    matches.push((i_a, idx, MatchType::Renamed));
                    break;
                }
            }
        }
    }

    //    3c. Best-effort similarity matching (≥ MODIFY_THRESHOLD).
    //    Indexed lookup by kind with subtree-size pre-filtering.
    for (i_a, na) in nodes_a.iter().enumerate() {
        if matched_a.contains(&na.id) {
            continue;
        }
        let size_a = na.subtree_size;
        let mut best: Option<(usize, f64)> = None;

        if let Some(candidates) = index_b.by_kind_with_size.get(na.kind.as_str()) {
            for &(idx, size_b) in candidates {
                let nb = &nodes_b[idx];
                if matched_b.contains(&nb.id) {
                    continue;
                }
                // Subtree-size pre-filter: skip if sizes differ by more than 3×
                let (smaller, larger) = if size_a <= size_b {
                    (size_a, size_b)
                } else {
                    (size_b, size_a)
                };
                if larger > 0 && smaller * 3 < larger {
                    continue;
                }
                let sim = node_identity::composite_similarity(&na.ast_node, &nb.ast_node);
                if sim >= node_identity::MODIFY_THRESHOLD && best.is_none_or(|(_, s)| sim > s) {
                    best = Some((idx, sim));
                }
            }
        }

        if let Some((idx, _)) = best {
            let nb = &nodes_b[idx];
            matched_a.insert(na.id);
            matched_b.insert(nb.id);
            matches.push((i_a, idx, MatchType::Modified));
        }
    }

    // ── Build operation records ──────────────────────────────────────
    let mut ops = Vec::with_capacity(matches.len() + nodes_a.len() + nodes_b.len());

    for &(idx_a, idx_b, ref match_type) in &matches {
        let na = &nodes_a[idx_a];
        let nb = &nodes_b[idx_b];
        let entity = classify_entity(&na.kind);
        let similarity = Some(semantic_similarity::compute_similarity(
            &na.ast_node,
            &nb.ast_node,
        ));
        match match_type {
            MatchType::Moved => {
                ops.push(OperationRecord {
                    op_type: OperationType::Move,
                    entity_type: entity,
                    old_location: Some(format_location(na)),
                    new_location: Some(format_location(nb)),
                    details: format!("{} '{}' moved", na.kind, na.name),
                    similarity,
                });
            }
            MatchType::Renamed => {
                ops.push(OperationRecord {
                    op_type: OperationType::Rename,
                    entity_type: entity,
                    old_location: Some(format_location(na)),
                    new_location: Some(format_location(nb)),
                    details: format!("{} renamed from '{}' to '{}'", na.kind, na.name, nb.name),
                    similarity,
                });
            }
            MatchType::Modified => {
                ops.push(OperationRecord {
                    op_type: OperationType::Modify,
                    entity_type: entity,
                    old_location: Some(format_location(na)),
                    new_location: Some(format_location(nb)),
                    details: format!("{} '{}' modified", na.kind, na.name),
                    similarity,
                });
            }
        }
    }

    // ── Phase 4: unmatched old nodes → DELETE ────────────────────────
    for na in &nodes_a {
        if !matched_a.contains(&na.id) {
            ops.push(OperationRecord {
                op_type: OperationType::Delete,
                entity_type: classify_entity(&na.kind),
                old_location: Some(format_location(na)),
                new_location: None,
                details: format!("{} '{}' deleted", na.kind, na.name),
                similarity: None,
            });
        }
    }

    // ── Phase 5: unmatched new nodes → INSERT ────────────────────────
    for nb in &nodes_b {
        if !matched_b.contains(&nb.id) {
            ops.push(OperationRecord {
                op_type: OperationType::Insert,
                entity_type: classify_entity(&nb.kind),
                old_location: None,
                new_location: Some(format_location(nb)),
                details: format!("{} '{}' inserted", nb.kind, nb.name),
                similarity: None,
            });
        }
    }

    ops
}

// ── Helpers ──────────────────────────────────────────────────────────

#[inline]
fn format_location(node: &SignificantNode) -> String {
    if node.start_row == node.end_row {
        format!("L{}", node.start_row + 1)
    } else {
        format!("L{}-L{}", node.start_row + 1, node.end_row + 1)
    }
}

/// Collect significant (top-level declarations, functions, classes, etc.)
/// nodes from the AST with their structural path.
fn collect_significant_nodes(node: &AstNode, parent_path: &[String]) -> Vec<SignificantNode> {
    let mut result = Vec::new();

    if is_significant_kind(&node.kind) {
        let name = extract_name(node);
        let mut current_path = Vec::with_capacity(parent_path.len() + 1);
        current_path.extend_from_slice(parent_path);
        current_path.push(format!("{}:{}", node.kind, name));

        result.push(SignificantNode {
            id: node.id,
            kind: node.kind.clone(),
            name,
            structural_hash: node.structural_hash,
            content_hash: node.content_hash,
            context_hash: node.context_hash,
            identity_hash: node.identity_hash,
            start_row: node.start_row,
            end_row: node.end_row,
            path: current_path.clone(),
            ast_node: node.clone(),
            subtree_size: count_nodes(node),
        });

        for child in &node.children {
            result.extend(collect_significant_nodes(child, &current_path));
        }
    } else {
        // Not significant — recurse with same parent path (no allocation)
        for child in &node.children {
            result.extend(collect_significant_nodes(child, parent_path));
        }
    }

    result
}

/// Try to extract a human-readable name from a node by looking at its
/// identifier children.
fn extract_name(node: &AstNode) -> String {
    // Direct children
    for child in &node.children {
        if is_name_bearing_kind(&child.kind) && !child.text.is_empty() {
            return child.text.clone();
        }
    }
    // One level deeper (e.g. Python where name is nested)
    for child in &node.children {
        for grandchild in &child.children {
            if is_name_bearing_kind(&grandchild.kind) && !grandchild.text.is_empty() {
                return grandchild.text.clone();
            }
        }
    }
    format!("anon@L{}", node.start_row + 1)
}

#[inline]
fn is_name_bearing_kind(kind: &str) -> bool {
    matches!(
        kind,
        "identifier"
            | "type_identifier"
            | "field_identifier"
            | "property_identifier"
            | "shorthand_field_identifier"
            | "name"
    )
}

/// Decide whether a tree-sitter node kind represents a structurally
/// significant code entity.
#[inline]
fn is_significant_kind(kind: &str) -> bool {
    matches!(
        kind,
        // Functions
        "function_item"
            | "function_definition"
            | "function_declaration"
            | "method_definition"
            | "method_declaration"
            | "arrow_function"
            | "closure_expression"
            | "lambda"
            // Classes / structs / traits / interfaces
            | "struct_item"
            | "enum_item"
            | "impl_item"
            | "trait_item"
            | "class_declaration"
            | "class_definition"
            | "interface_declaration"
            // Variables / constants
            | "let_declaration"
            | "const_item"
            | "static_item"
            | "variable_declaration"
            | "variable_declarator"
            | "lexical_declaration"
            | "const_declaration"
            | "assignment_statement"
            // Imports / exports
            | "use_declaration"
            | "import_statement"
            | "import_declaration"
            | "export_statement"
            // Type definitions
            | "type_alias"
            | "type_item"
            // Modules
            | "mod_item"
            | "module"
            // Decorators / annotations
            | "decorator"
            | "annotation"
    )
}

/// Map a tree-sitter node kind to our entity type taxonomy.
#[inline]
fn classify_entity(kind: &str) -> EntityType {
    match kind {
        "function_item"
        | "function_definition"
        | "function_declaration"
        | "method_definition"
        | "method_declaration"
        | "arrow_function"
        | "closure_expression"
        | "lambda" => EntityType::Function,

        "struct_item"
        | "enum_item"
        | "impl_item"
        | "trait_item"
        | "class_declaration"
        | "class_definition"
        | "interface_declaration" => EntityType::Class,

        "let_declaration"
        | "const_item"
        | "static_item"
        | "variable_declaration"
        | "variable_declarator"
        | "lexical_declaration"
        | "const_declaration"
        | "assignment_statement" => EntityType::Variable,

        "block" | "statement_block" => EntityType::Block,

        _ => EntityType::Other,
    }
}

// ── Whole-file INSERT / DELETE helpers ────────────────────────────────

fn collect_all_as_inserts(node: &AstNode) -> Vec<OperationRecord> {
    let nodes = collect_significant_nodes(node, &[]);
    nodes
        .into_iter()
        .map(|n| OperationRecord {
            op_type: OperationType::Insert,
            entity_type: classify_entity(&n.kind),
            old_location: None,
            new_location: Some(format_location(&n)),
            details: format!("{} '{}' inserted", n.kind, n.name),
            similarity: None,
        })
        .collect()
}

fn collect_all_as_deletes(node: &AstNode) -> Vec<OperationRecord> {
    let nodes = collect_significant_nodes(node, &[]);
    nodes
        .into_iter()
        .map(|n| OperationRecord {
            op_type: OperationType::Delete,
            entity_type: classify_entity(&n.kind),
            old_location: Some(format_location(&n)),
            new_location: None,
            details: format!("{} '{}' deleted", n.kind, n.name),
            similarity: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast_builder::parse_content;
    use crate::types::{AstNode, EntityType, OperationType, ParserLimits, SupportedLanguage};
    // ── Helpers ───────────────────────────────────────────────────────

    fn parse(src: &str, lang: SupportedLanguage) -> AstNode {
        parse_content(src, lang, false, &ParserLimits::default()).expect("parse failed")
    }

    fn count_op(ops: &[OperationRecord], t: &OperationType) -> usize {
        ops.iter().filter(|o| &o.op_type == t).count()
    }

    // ── count_nodes ───────────────────────────────────────────────────

    #[test]
    fn count_nodes_single() {
        let ast = parse("", SupportedLanguage::Rust);
        assert!(count_nodes(&ast) >= 1);
    }

    #[test]
    fn count_nodes_grows_with_source() {
        let small = parse("fn f() {}", SupportedLanguage::Rust);
        let big = parse(
            "fn f() { let x = 1; let y = 2; x }",
            SupportedLanguage::Rust,
        );
        assert!(count_nodes(&big) > count_nodes(&small));
    }

    // ── (None, None) → empty ──────────────────────────────────────────

    #[test]
    fn both_none_produces_no_ops() {
        let ops = compute_diff(None, None, false);
        assert!(ops.is_empty());
    }

    // ── (None, Some) → all INSERTs ────────────────────────────────────

    #[test]
    fn new_file_all_inserts() {
        let b = parse("fn greet() {}", SupportedLanguage::Rust);
        let ops = compute_diff(None, Some(&b), false);
        for op in &ops {
            assert_eq!(
                op.op_type,
                OperationType::Insert,
                "expected INSERT, got {:?}",
                op.op_type
            );
            assert!(op.old_location.is_none());
            assert!(op.new_location.is_some());
        }
    }

    // ── (Some, None) → all DELETEs ────────────────────────────────────

    #[test]
    fn deleted_file_all_deletes() {
        let a = parse("fn greet() {}", SupportedLanguage::Rust);
        let ops = compute_diff(Some(&a), None, false);
        for op in &ops {
            assert_eq!(
                op.op_type,
                OperationType::Delete,
                "expected DELETE, got {:?}",
                op.op_type
            );
            assert!(op.old_location.is_some());
            assert!(op.new_location.is_none());
        }
    }

    // ── Identical files → no ops ──────────────────────────────────────

    #[test]
    fn identical_files_no_ops() {
        let src = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let a = parse(src, SupportedLanguage::Rust);
        let b = parse(src, SupportedLanguage::Rust);
        let ops = compute_diff(Some(&a), Some(&b), false);
        assert!(
            ops.is_empty(),
            "identical files should produce no ops, got: {ops:?}"
        );
    }

    // ── INSERT: new function added ────────────────────────────────────

    #[test]
    fn added_function_produces_insert() {
        let a = parse("fn foo() {}", SupportedLanguage::Rust);
        let b = parse("fn foo() {}\nfn bar() {}", SupportedLanguage::Rust);
        let ops = compute_diff(Some(&a), Some(&b), false);
        let inserts = count_op(&ops, &OperationType::Insert);
        assert!(inserts >= 1, "expected at least one INSERT, ops: {ops:?}");
    }

    // ── DELETE: function removed ───────────────────────────────────────

    #[test]
    fn removed_function_produces_delete() {
        let a = parse("fn foo() {}\nfn bar() {}", SupportedLanguage::Rust);
        let b = parse("fn foo() {}", SupportedLanguage::Rust);
        let ops = compute_diff(Some(&a), Some(&b), false);
        let deletes = count_op(&ops, &OperationType::Delete);
        assert!(deletes >= 1, "expected at least one DELETE, ops: {ops:?}");
    }

    // ── MODIFY: function body changes ──────────────────────────────────

    #[test]
    fn modified_function_produces_modify() {
        let a = parse("fn compute() -> i32 { 1 + 1 }", SupportedLanguage::Rust);
        let b = parse("fn compute() -> i32 { 2 + 2 }", SupportedLanguage::Rust);
        let ops = compute_diff(Some(&a), Some(&b), false);
        let mods = count_op(&ops, &OperationType::Modify);
        assert!(mods >= 1, "expected at least one MODIFY, ops: {ops:?}");
    }

    // ── RENAME: function renamed (same body) ───────────────────────────

    #[test]
    fn renamed_function_produces_rename() {
        let a = parse("fn foo(x: i32) -> i32 { x + 1 }", SupportedLanguage::Rust);
        let b = parse("fn bar(x: i32) -> i32 { x + 1 }", SupportedLanguage::Rust);
        let ops = compute_diff(Some(&a), Some(&b), false);
        let renames = count_op(&ops, &OperationType::Rename);
        assert!(renames >= 1, "expected at least one RENAME, ops: {ops:?}");
    }

    // ── MOVE: function body stays, path changes ────────────────────────
    // (Verified by nesting a function inside a struct impl in version B)

    #[test]
    fn moved_function_produces_move() {
        // In A: top-level fn helper; In B: same fn inside impl block
        let a = parse("fn helper() -> i32 { 42 }", SupportedLanguage::Rust);
        let b = parse(
            "struct S; impl S { fn helper() -> i32 { 42 } }",
            SupportedLanguage::Rust,
        );
        let ops = compute_diff(Some(&a), Some(&b), false);
        let moves = count_op(&ops, &OperationType::Move);
        assert!(moves >= 1, "expected at least one MOVE, ops: {ops:?}");
    }

    // ── Python ────────────────────────────────────────────────────────

    #[test]
    fn python_new_function_insert() {
        let a = parse("def foo():\n    pass\n", SupportedLanguage::Python);
        let b = parse(
            "def foo():\n    pass\ndef bar():\n    pass\n",
            SupportedLanguage::Python,
        );
        let ops = compute_diff(Some(&a), Some(&b), false);
        assert!(count_op(&ops, &OperationType::Insert) >= 1, "ops: {ops:?}");
    }

    #[test]
    fn python_class_rename() {
        let a = parse("class OldName:\n    pass\n", SupportedLanguage::Python);
        let b = parse("class NewName:\n    pass\n", SupportedLanguage::Python);
        let ops = compute_diff(Some(&a), Some(&b), false);
        let renames = count_op(&ops, &OperationType::Rename);
        assert!(
            renames >= 1,
            "expected RENAME for Python class, ops: {ops:?}"
        );
    }

    // ── JavaScript ───────────────────────────────────────────────────

    #[test]
    fn js_function_delete() {
        let a = parse(
            "function alpha() {} function beta() {}",
            SupportedLanguage::JavaScript,
        );
        let b = parse("function alpha() {}", SupportedLanguage::JavaScript);
        let ops = compute_diff(Some(&a), Some(&b), false);
        assert!(count_op(&ops, &OperationType::Delete) >= 1, "ops: {ops:?}");
    }

    // ── Java ──────────────────────────────────────────────────────────

    #[test]
    fn java_method_modify() {
        let a = parse(
            "class C { void run() { int x = 1; } }",
            SupportedLanguage::Java,
        );
        let b = parse(
            "class C { void run() { int x = 99; } }",
            SupportedLanguage::Java,
        );
        let ops = compute_diff(Some(&a), Some(&b), false);
        assert!(count_op(&ops, &OperationType::Modify) >= 1, "ops: {ops:?}");
    }

    // ── Insert details ────────────────────────────────────────────────

    #[test]
    fn insert_op_contains_detail_string() {
        let b = parse("fn greet() {}", SupportedLanguage::Rust);
        let ops = compute_diff(None, Some(&b), false);
        for op in &ops {
            assert!(!op.details.is_empty(), "details should not be empty");
        }
    }

    // ── Delete details ────────────────────────────────────────────────

    #[test]
    fn delete_op_contains_detail_string() {
        let a = parse("fn greet() {}", SupportedLanguage::Rust);
        let ops = compute_diff(Some(&a), None, false);
        for op in &ops {
            assert!(!op.details.is_empty());
        }
    }

    // ── Entity classification ─────────────────────────────────────────

    #[test]
    fn inserted_rust_function_has_function_entity() {
        let b = parse("fn my_func() {}", SupportedLanguage::Rust);
        let ops = compute_diff(None, Some(&b), false);
        let func_ops: Vec<_> = ops
            .iter()
            .filter(|o| o.entity_type == EntityType::Function)
            .collect();
        assert!(
            !func_ops.is_empty(),
            "expected at least one Function entity, ops: {ops:?}"
        );
    }

    #[test]
    fn inserted_rust_struct_has_class_entity() {
        let b = parse("struct Point { x: f64, y: f64 }", SupportedLanguage::Rust);
        let ops = compute_diff(None, Some(&b), false);
        let class_ops: Vec<_> = ops
            .iter()
            .filter(|o| o.entity_type == EntityType::Class)
            .collect();
        assert!(
            !class_ops.is_empty(),
            "expected Class entity for struct, ops: {ops:?}"
        );
    }

    // ── Rename / modify priority ──────────────────────────────────────

    #[test]
    fn body_change_not_classified_as_rename() {
        // Same name, different body → should be MODIFY not RENAME
        let a = parse("fn process() -> i32 { 10 }", SupportedLanguage::Rust);
        let b = parse("fn process() -> i32 { 99 }", SupportedLanguage::Rust);
        let ops = compute_diff(Some(&a), Some(&b), false);
        let renames = count_op(&ops, &OperationType::Rename);
        assert_eq!(
            renames, 0,
            "body change should not be classified as RENAME, ops: {ops:?}"
        );
        assert!(count_op(&ops, &OperationType::Modify) >= 1);
    }

    // ── logic_only flag ───────────────────────────────────────────────

    #[test]
    fn comment_only_change_produces_no_ops_in_logic_only() {
        let a_src = "fn work() {\n    let x = 1;\n}";
        let b_src = "fn work() {\n    // a new comment\n    let x = 1;\n}";
        let a = parse_content(
            a_src,
            SupportedLanguage::Rust,
            true,
            &ParserLimits::default(),
        )
        .unwrap();
        let b = parse_content(
            b_src,
            SupportedLanguage::Rust,
            true,
            &ParserLimits::default(),
        )
        .unwrap();
        let ops = compute_diff(Some(&a), Some(&b), true);
        assert!(
            ops.is_empty(),
            "comment-only change should be transparent in logic_only mode, ops: {ops:?}"
        );
    }
}
