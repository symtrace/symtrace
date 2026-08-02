//! Cross-File Symbol Tracking
//!
//! Builds a global symbol table from all parsed files and detects
//! cross-file events:
//!
//! * **cross_file_move** — a symbol with the same `structure_hash` and
//!   `signature_hash` appears in a different file.
//! * **cross_file_rename** — a symbol's `structure_hash` matches across
//!   files but the name differs (identity hash still matches).
//! * **api_surface_change** — a public symbol's signature changed across
//!   files (detected via high similarity + name match but different
//!   `signature_hash`).

use crate::node_identity;
use crate::types::{
    AstNode, CrossFileEventKind, CrossFileMatch, CrossFileTracking, EntityType, SymbolEntry,
    SymbolId,
};

// ── Thresholds ───────────────────────────────────────────────────────

/// Minimum similarity score for cross-file rename detection.
const CROSS_FILE_RENAME_THRESHOLD: f64 = 0.85;
/// Minimum similarity score for API surface change detection.
const API_SURFACE_THRESHOLD: f64 = 0.60;

// ── Public API ───────────────────────────────────────────────────────

/// Build a global symbol table from all parsed file ASTs and detect
/// cross-file symbol events between old and new versions.
///
/// `parsed_pairs` is a slice of `(file_path, old_ast, new_ast)`.
pub fn track_cross_file_symbols(
    parsed_pairs: &[(String, Option<AstNode>, Option<AstNode>)],
) -> CrossFileTracking {
    // Build symbol tables for old and new state
    let mut old_symbols: Vec<SymbolEntry> = Vec::new();
    let mut new_symbols: Vec<SymbolEntry> = Vec::new();
    let mut id_counter: SymbolId = 1;

    for (path, old_ast, new_ast) in parsed_pairs {
        if let Some(ast) = old_ast {
            collect_symbols(ast, path, 0, &mut old_symbols, &mut id_counter);
        }
        if let Some(ast) = new_ast {
            collect_symbols(ast, path, 0, &mut new_symbols, &mut id_counter);
        }
    }

    let total_symbol_count = old_symbols.len() + new_symbols.len();

    // Detect cross-file events
    let events = detect_cross_file_events(&old_symbols, &new_symbols, parsed_pairs);

    CrossFileTracking {
        symbol_count: total_symbol_count,
        cross_file_events: events,
    }
}

// ── Symbol Collection ────────────────────────────────────────────────

/// Recursively collect significant symbols from an AST into the symbol table.
fn collect_symbols(
    node: &AstNode,
    file_path: &str,
    parent_id: SymbolId,
    symbols: &mut Vec<SymbolEntry>,
    id_counter: &mut SymbolId,
) {
    if let Some(entity_type) = classify_symbol_entity(&node.kind) {
        let name = extract_symbol_name(node);
        let sig_hash = compute_signature_hash(node);
        let sym_id = *id_counter;
        *id_counter += 1;

        symbols.push(SymbolEntry {
            symbol_id: sym_id,
            name,
            file_path: file_path.to_string(),
            entity_type,
            signature_hash: sig_hash,
            structure_hash: node.structural_hash,
            parent_symbol_id: parent_id,
        });

        // Recurse into children with this symbol as parent
        for child in &node.children {
            collect_symbols(child, file_path, sym_id, symbols, id_counter);
        }
    } else {
        // Not a symbol-bearing node, recurse with same parent
        for child in &node.children {
            collect_symbols(child, file_path, parent_id, symbols, id_counter);
        }
    }
}

/// Compute a signature hash: blake3(kind + name + structural_hash).
/// This captures the "API surface" of a symbol.
fn compute_signature_hash(node: &AstNode) -> [u8; 32] {
    let name = extract_symbol_name(node);
    let mut hasher = blake3::Hasher::new();
    hasher.update(node.kind.as_bytes());
    hasher.update(name.as_bytes());
    hasher.update(&node.structural_hash);
    *hasher.finalize().as_bytes()
}

/// Extract a human-readable name from a node by looking at identifier children using BFS.
fn extract_symbol_name(node: &AstNode) -> String {
    let mut queue = std::collections::VecDeque::new();
    for child in &node.children {
        queue.push_back((child, 1usize));
    }

    while let Some((curr, depth)) = queue.pop_front() {
        if node_identity::is_identifier_kind(&curr.kind) && !curr.text.is_empty() {
            return curr.text.clone();
        }
        if depth < 5 {
            for child in &curr.children {
                if classify_symbol_entity(&child.kind).is_none() {
                    queue.push_back((child, depth + 1));
                }
            }
        }
    }

    format!("anon@L{}", node.start_row + 1)
}

/// Classify a node kind as a trackable symbol entity type, or None.
fn classify_symbol_entity(kind: &str) -> Option<EntityType> {
    match kind {
        "function_item"
        | "function_definition"
        | "function_declaration"
        | "method_definition"
        | "method_declaration"
        | "arrow_function"
        | "closure_expression"
        | "lambda" => Some(EntityType::Function),

        "struct_item"
        | "enum_item"
        | "impl_item"
        | "trait_item"
        | "class_declaration"
        | "class_definition"
        | "interface_declaration"
        | "struct_specifier"
        | "enum_specifier"
        | "type_definition"
        | "type_spec"
        | "type_declaration"
        | "class_specifier"
        | "namespace_definition"
        | "template_declaration" => Some(EntityType::Class),

        "let_declaration"
        | "const_item"
        | "static_item"
        | "variable_declaration"
        | "variable_declarator"
        | "lexical_declaration"
        | "const_declaration"
        | "assignment_statement"
        | "pair"
        | "short_var_declaration" => Some(EntityType::Variable),

        _ => None,
    }
}

// ── Cross-File Event Detection ───────────────────────────────────────

/// Detect cross-file events by comparing old symbols against new symbols.
fn detect_cross_file_events(
    old_symbols: &[SymbolEntry],
    new_symbols: &[SymbolEntry],
    parsed_pairs: &[(String, Option<AstNode>, Option<AstNode>)],
) -> Vec<CrossFileMatch> {
    let mut events = Vec::new();

    // We only look for symbols that disappeared from one file and appeared in another.
    // For each old symbol, try to find a matching new symbol in a DIFFERENT file.

    // Track which new symbols have been matched
    let mut matched_new: Vec<bool> = vec![false; new_symbols.len()];

    for old_sym in old_symbols {
        // Check if this symbol still exists in its own file in the new version
        let still_in_same_file = new_symbols.iter().any(|ns| {
            ns.file_path == old_sym.file_path
                && ns.name == old_sym.name
                && ns.structure_hash == old_sym.structure_hash
        });
        if still_in_same_file {
            continue;
        }

        // ── 1. Cross-file move: same structure_hash + signature_hash, different file
        for (ni, new_sym) in new_symbols.iter().enumerate() {
            if matched_new[ni] || new_sym.file_path == old_sym.file_path {
                continue;
            }
            if new_sym.structure_hash == old_sym.structure_hash
                && new_sym.signature_hash == old_sym.signature_hash
            {
                matched_new[ni] = true;
                events.push(CrossFileMatch {
                    event: CrossFileEventKind::CrossFileMove,
                    old_symbol: old_sym.name.clone(),
                    old_file: old_sym.file_path.clone(),
                    new_symbol: new_sym.name.clone(),
                    new_file: new_sym.file_path.clone(),
                    similarity_score: 1.0,
                    description: format!(
                        "{} '{}' moved from '{}' to '{}'",
                        old_sym.entity_type, old_sym.name, old_sym.file_path, new_sym.file_path
                    ),
                });
                break;
            }
        }

        // ── 2. Cross-file rename: same structure_hash, different name, different file
        for (ni, new_sym) in new_symbols.iter().enumerate() {
            if matched_new[ni] || new_sym.file_path == old_sym.file_path {
                continue;
            }
            if new_sym.structure_hash == old_sym.structure_hash
                && new_sym.name != old_sym.name
                && new_sym.entity_type == old_sym.entity_type
            {
                // Compute similarity using AST nodes
                let sim = compute_cross_file_similarity(old_sym, new_sym, parsed_pairs);
                if sim >= CROSS_FILE_RENAME_THRESHOLD {
                    matched_new[ni] = true;
                    events.push(CrossFileMatch {
                        event: CrossFileEventKind::CrossFileRename,
                        old_symbol: old_sym.name.clone(),
                        old_file: old_sym.file_path.clone(),
                        new_symbol: new_sym.name.clone(),
                        new_file: new_sym.file_path.clone(),
                        similarity_score: sim,
                        description: format!(
                            "{} '{}' in '{}' renamed to '{}' in '{}'",
                            old_sym.entity_type,
                            old_sym.name,
                            old_sym.file_path,
                            new_sym.name,
                            new_sym.file_path,
                        ),
                    });
                    break;
                }
            }
        }

        // ── 3. API surface change: same name, different file, different signature
        for (ni, new_sym) in new_symbols.iter().enumerate() {
            if matched_new[ni] || new_sym.file_path == old_sym.file_path {
                continue;
            }
            if new_sym.name == old_sym.name
                && new_sym.entity_type == old_sym.entity_type
                && new_sym.signature_hash != old_sym.signature_hash
            {
                let sim = compute_cross_file_similarity(old_sym, new_sym, parsed_pairs);
                if sim >= API_SURFACE_THRESHOLD {
                    matched_new[ni] = true;
                    events.push(CrossFileMatch {
                        event: CrossFileEventKind::ApiSurfaceChange,
                        old_symbol: old_sym.name.clone(),
                        old_file: old_sym.file_path.clone(),
                        new_symbol: new_sym.name.clone(),
                        new_file: new_sym.file_path.clone(),
                        similarity_score: sim,
                        description: format!(
                            "{} '{}' API changed when moving from '{}' to '{}'",
                            old_sym.entity_type, old_sym.name, old_sym.file_path, new_sym.file_path,
                        ),
                    });
                    break;
                }
            }
        }
    }

    events
}

/// Compute cross-file similarity by finding the AST nodes matching the
/// given symbols and running the composite similarity scorer.
fn compute_cross_file_similarity(
    old_sym: &SymbolEntry,
    new_sym: &SymbolEntry,
    parsed_pairs: &[(String, Option<AstNode>, Option<AstNode>)],
) -> f64 {
    let old_node = find_ast_node_for_symbol(old_sym, parsed_pairs, true);
    let new_node = find_ast_node_for_symbol(new_sym, parsed_pairs, false);

    match (old_node, new_node) {
        (Some(a), Some(b)) => node_identity::composite_similarity(a, b),
        _ => 0.0,
    }
}

/// Find the AST node in the parsed pairs that corresponds to a symbol.
/// `use_old` selects whether to look in old_ast (true) or new_ast (false).
fn find_ast_node_for_symbol<'a>(
    sym: &SymbolEntry,
    parsed_pairs: &'a [(String, Option<AstNode>, Option<AstNode>)],
    use_old: bool,
) -> Option<&'a AstNode> {
    for (path, old_ast, new_ast) in parsed_pairs {
        if path != &sym.file_path {
            continue;
        }
        let ast = if use_old {
            old_ast.as_ref()
        } else {
            new_ast.as_ref()
        };
        if let Some(root) = ast {
            if let Some(node) = find_node_by_name_and_kind(root, &sym.name, &sym.structure_hash) {
                return Some(node);
            }
        }
    }
    None
}

/// Recursively find a node with a matching name and structure hash.
fn find_node_by_name_and_kind<'a>(
    node: &'a AstNode,
    name: &str,
    structure_hash: &[u8; 32],
) -> Option<&'a AstNode> {
    if node.structural_hash == *structure_hash {
        let node_name = extract_symbol_name(node);
        if node_name == name {
            return Some(node);
        }
    }
    for child in &node.children {
        if let Some(found) = find_node_by_name_and_kind(child, name, structure_hash) {
            return Some(found);
        }
    }
    None
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast_builder::parse_content;
    use crate::types::{ParserLimits, SupportedLanguage};
    fn parse(src: &str, lang: SupportedLanguage) -> AstNode {
        parse_content(src, lang, false, &ParserLimits::default()).expect("parse failed")
    }

    #[test]
    fn empty_input_produces_empty_tracking() {
        let pairs: Vec<(String, Option<AstNode>, Option<AstNode>)> = vec![];
        let result = track_cross_file_symbols(&pairs);
        assert_eq!(result.symbol_count, 0);
        assert!(result.cross_file_events.is_empty());
    }

    #[test]
    fn single_file_no_cross_file_events() {
        let old = parse("fn foo() { let x = 1; }", SupportedLanguage::Rust);
        let new = parse("fn foo() { let x = 2; }", SupportedLanguage::Rust);
        let pairs = vec![("src/main.rs".to_string(), Some(old), Some(new))];
        let result = track_cross_file_symbols(&pairs);
        assert!(result.symbol_count > 0);
        // No cross-file events since only one file
        assert!(
            result.cross_file_events.is_empty(),
            "single file should not produce cross-file events, got: {:?}",
            result.cross_file_events
        );
    }

    #[test]
    fn cross_file_move_detected() {
        // Function 'helper' exists in file_a old, and in file_b new (identical body)
        let old_a = parse("fn helper() -> i32 { 42 }", SupportedLanguage::Rust);
        let new_b = parse("fn helper() -> i32 { 42 }", SupportedLanguage::Rust);
        let pairs = vec![
            (
                "src/file_a.rs".to_string(),
                Some(old_a),
                None, // deleted from file_a
            ),
            (
                "src/file_b.rs".to_string(),
                None, // didn't exist before
                Some(new_b),
            ),
        ];
        let result = track_cross_file_symbols(&pairs);
        let moves: Vec<_> = result
            .cross_file_events
            .iter()
            .filter(|e| e.event == CrossFileEventKind::CrossFileMove)
            .collect();
        assert!(
            !moves.is_empty(),
            "should detect cross-file move, events: {:?}",
            result.cross_file_events
        );
        assert_eq!(moves[0].old_file, "src/file_a.rs");
        assert_eq!(moves[0].new_file, "src/file_b.rs");
    }

    #[test]
    fn cross_file_rename_detected() {
        // Same function body, different name, different file
        let old_a = parse(
            "fn old_name(x: i32) -> i32 { x + 1 }",
            SupportedLanguage::Rust,
        );
        let new_b = parse(
            "fn new_name(x: i32) -> i32 { x + 1 }",
            SupportedLanguage::Rust,
        );
        let pairs = vec![
            ("src/file_a.rs".to_string(), Some(old_a), None),
            ("src/file_b.rs".to_string(), None, Some(new_b)),
        ];
        let result = track_cross_file_symbols(&pairs);
        let renames: Vec<_> = result
            .cross_file_events
            .iter()
            .filter(|e| e.event == CrossFileEventKind::CrossFileRename)
            .collect();
        assert!(
            !renames.is_empty(),
            "should detect cross-file rename, events: {:?}",
            result.cross_file_events
        );
    }

    #[test]
    fn symbol_table_counts_all_symbols() {
        let a = parse("fn foo() {}\nfn bar() {}", SupportedLanguage::Rust);
        let b = parse("fn baz() {}", SupportedLanguage::Rust);
        let pairs = vec![
            ("a.rs".to_string(), Some(a), None),
            ("b.rs".to_string(), None, Some(b)),
        ];
        let result = track_cross_file_symbols(&pairs);
        // 2 symbols from old file + 1 from new file = 3
        assert!(
            result.symbol_count >= 3,
            "expected at least 3 symbols, got {}",
            result.symbol_count
        );
    }

    #[test]
    fn classify_symbol_entity_covers_all_types() {
        assert_eq!(
            classify_symbol_entity("function_item"),
            Some(EntityType::Function)
        );
        assert_eq!(
            classify_symbol_entity("class_declaration"),
            Some(EntityType::Class)
        );
        assert_eq!(
            classify_symbol_entity("let_declaration"),
            Some(EntityType::Variable)
        );
        assert_eq!(classify_symbol_entity("source_file"), None);
    }

    #[test]
    fn cross_file_event_display() {
        assert_eq!(
            CrossFileEventKind::CrossFileMove.to_string(),
            "cross_file_move"
        );
        assert_eq!(
            CrossFileEventKind::CrossFileRename.to_string(),
            "cross_file_rename"
        );
        assert_eq!(
            CrossFileEventKind::ApiSurfaceChange.to_string(),
            "api_surface_change"
        );
    }

    #[test]
    fn no_false_positive_when_symbol_stays() {
        // Symbol exists in same file in both versions — no cross-file event
        let old = parse("fn helper() -> i32 { 42 }", SupportedLanguage::Rust);
        let new = parse("fn helper() -> i32 { 42 }", SupportedLanguage::Rust);
        let pairs = vec![("src/lib.rs".to_string(), Some(old), Some(new))];
        let result = track_cross_file_symbols(&pairs);
        assert!(
            result.cross_file_events.is_empty(),
            "symbol staying in same file should not trigger cross-file events, got: {:?}",
            result.cross_file_events
        );
    }
}
