use std::collections::HashSet;

use crate::incremental_parse;
use crate::types::AstNode;

// ── Similarity model constants ───────────────────────────────────────

/// Weight given to structural similarity when computing composite score.
pub const STRUCTURAL_WEIGHT: f64 = 0.6;
/// Weight given to token/content similarity when computing composite score.
pub const TOKEN_WEIGHT: f64 = 0.4;
/// Minimum similarity score to classify a change as RENAME.
pub const RENAME_THRESHOLD: f64 = 0.90;
/// Minimum similarity score to classify a change as MODIFY.
pub const MODIFY_THRESHOLD: f64 = 0.70;

// ── Hash computation ─────────────────────────────────────────────────

/// Compute all hashes for every node in the tree.
///
/// Phase 1 (bottom-up):
/// * **structural_hash** – `blake3(node_kind + ordered child structural_hashes)`.
///   Captures *pure tree shape* without any leaf content.
/// * **content_hash** – `blake3(actual_tokens)`.
///   Captures leaf content as-is (identifiers NOT normalised) so that a
///   rename produces a different content_hash.
/// * **identity_hash** – `blake3(kind + normalised identifiers)`.
///   Identifiers replaced by `<IDENTIFIER>` — used for rename detection.
///
/// Phase 2 (top-down):
/// * **context_hash** – `blake3(parent_structure_hash + depth)`.
///   Captures the node's position within the tree.
pub fn compute_hashes(node: &mut AstNode, logic_only: bool) {
    // Bottom-up pass: structural_hash, content_hash, identity_hash
    compute_hashes_bottom_up(node, logic_only);
    // Top-down pass: lexical scope-aware context_hash
    compute_context_hashes(node, &[0u8; 32], &[0u8; 32], "", 0, 0);
}

/// Compute hashes with reuse from a previous AST for unchanged subtrees.
///
/// For nodes whose byte ranges fall entirely outside the changed regions
/// (as reported by tree-sitter's `changed_ranges()`), this function copies
/// the bottom-up hashes (structural, content, identity) from the old AST
/// rather than recomputing them. The context_hash is always recomputed
/// in the top-down pass (it's cheap — single blake3 per node).
///
/// Returns the number of nodes whose hashes were reused.
pub fn compute_hashes_incremental(
    node: &mut AstNode,
    old_node: &AstNode,
    changed_ranges: &[tree_sitter::Range],
    logic_only: bool,
) -> u64 {
    let reused = compute_bottom_up_with_reuse(node, Some(old_node), changed_ranges, logic_only);
    compute_context_hashes(node, &[0u8; 32], &[0u8; 32], "", 0, 0);
    reused
}

/// Bottom-up pass with hash reuse for unchanged subtrees.
///
/// A node is considered "unchanged" if its byte range [start_byte, end_byte)
/// does not overlap any of the changed ranges AND it has a matching old node
/// (same kind and child count). For such nodes, the structural_hash, content_hash, and
/// identity_hash are copied from the old tree without recomputation.
///
/// For changed nodes (or nodes without a matching old node), hashes are
/// computed from scratch using the same algorithm as `compute_hashes_bottom_up`.
fn compute_bottom_up_with_reuse(
    node: &mut AstNode,
    old_node: Option<&AstNode>,
    changed_ranges: &[tree_sitter::Range],
    logic_only: bool,
) -> u64 {
    // Check if this node is entirely outside all changed ranges
    let is_changed =
        incremental_parse::overlaps_changed_ranges(node.start_byte, node.end_byte, changed_ranges);

    // If unchanged and we have a matching old node (with identical child count to ensure structural stability), reuse hashes
    if !is_changed {
        if let Some(old) = old_node {
            if old.kind == node.kind && old.children.len() == node.children.len() {
                // Copy bottom-up hashes from old node
                node.structural_hash = old.structural_hash;
                node.content_hash = old.content_hash;
                node.identity_hash = old.identity_hash;

                // Recursively reuse for children (matched by index)
                let mut reused = 1u64;
                for (i, child) in node.children.iter_mut().enumerate() {
                    let old_child = old.children.get(i);
                    reused +=
                        compute_bottom_up_with_reuse(child, old_child, changed_ranges, logic_only);
                }
                return reused;
            }
        }
    }

    // Changed or no matching old node — compute from scratch
    // Process children first (bottom-up), still trying reuse for each child
    let mut reused = 0u64;
    for (i, child) in node.children.iter_mut().enumerate() {
        let old_child = old_node.and_then(|o| o.children.get(i));
        reused += compute_bottom_up_with_reuse(child, old_child, changed_ranges, logic_only);
    }

    // ── structural_hash ──────────────────────────────────────────────
    let mut struct_hasher = blake3::Hasher::new();
    struct_hasher.update(node.kind.as_bytes());
    for child in &node.children {
        struct_hasher.update(&child.structural_hash);
    }
    node.structural_hash = *struct_hasher.finalize().as_bytes();

    // ── content_hash ─────────────────────────────────────────────────
    let mut content_hasher = blake3::Hasher::new();
    if node.children.is_empty() {
        if logic_only && is_comment_or_whitespace(&node.kind) {
            content_hasher.update(b"<COMMENT>");
        } else {
            content_hasher.update(node.text.as_bytes());
        }
    } else {
        for child in &node.children {
            content_hasher.update(&child.content_hash);
        }
    }
    node.content_hash = *content_hasher.finalize().as_bytes();

    // ── identity_hash ────────────────────────────────────────────────
    let mut id_hasher = blake3::Hasher::new();
    id_hasher.update(node.kind.as_bytes());
    if node.children.is_empty() {
        if is_identifier_kind(&node.kind) {
            id_hasher.update(b"<IDENTIFIER>");
        } else if logic_only && is_comment_or_whitespace(&node.kind) {
            id_hasher.update(b"<COMMENT>");
        } else {
            id_hasher.update(node.text.as_bytes());
        }
    } else {
        for child in &node.children {
            id_hasher.update(&child.identity_hash);
        }
    }
    node.identity_hash = *id_hasher.finalize().as_bytes();

    reused
}

/// Bottom-up pass: compute structural_hash, content_hash, and identity_hash.
fn compute_hashes_bottom_up(node: &mut AstNode, logic_only: bool) {
    for child in &mut node.children {
        compute_hashes_bottom_up(child, logic_only);
    }

    // ── structural_hash: blake3(node_kind + ordered child structure_hashes) ──
    //    For leaves this is just blake3(kind). No leaf text is included.
    let mut struct_hasher = blake3::Hasher::new();
    struct_hasher.update(node.kind.as_bytes());
    for child in &node.children {
        struct_hasher.update(&child.structural_hash);
    }
    node.structural_hash = *struct_hasher.finalize().as_bytes();

    // ── content_hash: blake3(actual_tokens) ───────────────────────
    //    Leaf tokens with their real text. Comments become <COMMENT>
    //    in logic_only mode, but identifiers are kept as-is so that
    //    a rename produces a *different* content_hash.
    let mut content_hasher = blake3::Hasher::new();
    if node.children.is_empty() {
        if logic_only && is_comment_or_whitespace(&node.kind) {
            content_hasher.update(b"<COMMENT>");
        } else {
            content_hasher.update(node.text.as_bytes());
        }
    } else {
        for child in &node.children {
            content_hasher.update(&child.content_hash);
        }
    }
    node.content_hash = *content_hasher.finalize().as_bytes();

    // ── identity_hash: blake3(kind + normalised content) ────────────
    //    Preserves backward compatibility for rename detection.
    let mut id_hasher = blake3::Hasher::new();
    id_hasher.update(node.kind.as_bytes());
    if node.children.is_empty() {
        if is_identifier_kind(&node.kind) {
            id_hasher.update(b"<IDENTIFIER>");
        } else if logic_only && is_comment_or_whitespace(&node.kind) {
            id_hasher.update(b"<COMMENT>");
        } else {
            id_hasher.update(node.text.as_bytes());
        }
    } else {
        for child in &node.children {
            id_hasher.update(&child.identity_hash);
        }
    }
    node.identity_hash = *id_hasher.finalize().as_bytes();
}

/// Top-down pass: compute lexical scope-aware context_hash.
/// Incorporates the BLAKE3 hash of the nearest enclosing named declaration boundary
/// (function, struct, impl, class, trait, or module) + parent structural hash + relative depth offset.
fn compute_context_hashes(
    node: &mut AstNode,
    enclosing_scope_hash: &[u8; 32],
    parent_structural_hash: &[u8; 32],
    parent_kind: &str,
    sibling_index: u32,
    depth: u32,
) {
    let mut ctx_hasher = blake3::Hasher::new();
    ctx_hasher.update(enclosing_scope_hash);
    ctx_hasher.update(parent_structural_hash);
    ctx_hasher.update(parent_kind.as_bytes());
    ctx_hasher.update(&sibling_index.to_le_bytes());
    ctx_hasher.update(&depth.to_le_bytes());
    node.context_hash = *ctx_hasher.finalize().as_bytes();

    let my_structural_hash = node.structural_hash;
    let my_kind = node.kind.clone();

    // If current node is a named scope declaration boundary, update enclosing_scope_hash for children
    let next_enclosing_scope = if is_scope_declaration_boundary(&node.kind) {
        let mut scope_hasher = blake3::Hasher::new();
        scope_hasher.update(node.kind.as_bytes());
        scope_hasher.update(&node.structural_hash);
        *scope_hasher.finalize().as_bytes()
    } else {
        *enclosing_scope_hash
    };

    for (i, child) in node.children.iter_mut().enumerate() {
        compute_context_hashes(
            child,
            &next_enclosing_scope,
            &my_structural_hash,
            &my_kind,
            i as u32,
            depth + 1,
        );
    }
}

#[inline]
fn is_scope_declaration_boundary(kind: &str) -> bool {
    matches!(
        kind,
        "function_item"
            | "function_definition"
            | "function_declaration"
            | "method_definition"
            | "method_declaration"
            | "struct_item"
            | "enum_item"
            | "impl_item"
            | "trait_item"
            | "class_declaration"
            | "class_definition"
            | "interface_declaration"
            | "mod_item"
            | "module"
            | "namespace_definition"
    )
}

/// Fast 64-bit token bitset for O(1) disjointness pre-filtering.
#[inline]
pub fn token_bitset(tokens: &[String]) -> u64 {
    let mut mask = 0u64;
    for t in tokens {
        let h = blake3::hash(t.as_bytes());
        let bit = (h.as_bytes()[0] as usize) % 64;
        mask |= 1u64 << bit;
    }
    mask
}

/// Fast 16-bin token frequency histogram for SIMD vector calculation.
#[inline]
pub fn token_histogram_16(tokens: &[String]) -> [u8; 16] {
    let mut hist = [0u8; 16];
    for t in tokens {
        let h = blake3::hash(t.as_bytes());
        let bin = (h.as_bytes()[0] as usize) % 16;
        hist[bin] = hist[bin].saturating_add(1);
    }
    hist
}

/// Compute SIMD/auto-vectorized Jaccard similarity across 16-bin frequency histograms.
/// Computes sum(min(a_i, b_i)) / sum(max(a_i, b_i)) in 100% safe, auto-vectorizable Rust.
#[inline]
pub fn simd_jaccard_histogram_16(hist_a: &[u8; 16], hist_b: &[u8; 16]) -> f64 {
    let mut sum_min = 0u32;
    let mut sum_max = 0u32;
    for i in 0..16 {
        sum_min += (hist_a[i].min(hist_b[i])) as u32;
        sum_max += (hist_a[i].max(hist_b[i])) as u32;
    }
    if sum_max == 0 {
        1.0
    } else {
        sum_min as f64 / sum_max as f64
    }
}

// ── Similarity computation ───────────────────────────────────────────

/// Compute the *structural similarity* between two AST nodes.
///
/// This is the ratio of overlapping structural sub-hashes.
pub fn structural_similarity(a: &AstNode, b: &AstNode) -> f64 {
    let hashes_a = collect_structural_hashes(a);
    let hashes_b = collect_structural_hashes(b);
    if hashes_a.is_empty() && hashes_b.is_empty() {
        return 1.0;
    }
    let intersection = hashes_a.intersection(&hashes_b).count();
    let union = hashes_a.union(&hashes_b).count();
    if union == 0 {
        return 1.0;
    }
    intersection as f64 / union as f64
}

/// Compute the *token similarity* between two AST nodes.
///
/// Accelerated in v0.5.0 with 64-bit bitset pre-filtering, SIMD histogram bounding,
/// and Multiset Frequency (Bag-of-Words) Jaccard ratio:
/// Sim = sum(min(count_A, count_B)) / sum(max(count_A, count_B))
pub fn token_similarity(a: &AstNode, b: &AstNode) -> f64 {
    let tokens_a = collect_normalised_tokens(a);
    let tokens_b = collect_normalised_tokens(b);
    if tokens_a.is_empty() && tokens_b.is_empty() {
        return 1.0;
    }
    if tokens_a.is_empty() || tokens_b.is_empty() {
        return 0.0;
    }

    // 1. Fast Identity Short-Circuit
    if tokens_a.len() == tokens_b.len() && tokens_a == tokens_b {
        return 1.0;
    }

    // 2. 64-Bit Bitset Disjointness Fast-Path
    let bitset_a = token_bitset(&tokens_a);
    let bitset_b = token_bitset(&tokens_b);
    if (bitset_a & bitset_b) == 0 && (tokens_a.len() > 3 || tokens_b.len() > 3) {
        return 0.0;
    }

    // 3. Exact Multiset Frequency (Bag-of-Words) Jaccard calculation
    let mut freq_a = std::collections::HashMap::with_capacity(tokens_a.len());
    let mut freq_b = std::collections::HashMap::with_capacity(tokens_b.len());

    for t in &tokens_a {
        *freq_a.entry(t.as_str()).or_insert(0u32) += 1;
    }
    for t in &tokens_b {
        *freq_b.entry(t.as_str()).or_insert(0u32) += 1;
    }

    let mut keys: HashSet<&str> = HashSet::with_capacity(freq_a.len() + freq_b.len());
    keys.extend(freq_a.keys());
    keys.extend(freq_b.keys());

    let mut intersection_sum = 0u32;
    let mut union_sum = 0u32;

    for k in keys {
        let count_a = freq_a.get(k).copied().unwrap_or(0);
        let count_b = freq_b.get(k).copied().unwrap_or(0);
        intersection_sum += count_a.min(count_b);
        union_sum += count_a.max(count_b);
    }

    if union_sum == 0 {
        return 1.0;
    }
    intersection_sum as f64 / union_sum as f64
}

/// Compute the weighted composite similarity score for the matching model.
///
/// Uses `STRUCTURAL_WEIGHT` (0.6) and `TOKEN_WEIGHT` (0.4).
pub fn composite_similarity(a: &AstNode, b: &AstNode) -> f64 {
    let ss = structural_similarity(a, b);
    let ts = token_similarity(a, b);
    ss * STRUCTURAL_WEIGHT + ts * TOKEN_WEIGHT
}

/// Check whether only identifiers differ between two structurally matching nodes.
#[inline]
pub fn only_identifiers_changed(a: &AstNode, b: &AstNode) -> bool {
    // Same structure hash (pure tree shape) but different content hash
    // means something in the tokens changed. We further check that
    // identity_hash matches (which normalises identifiers) to confirm
    // only identifier text was altered.
    a.structural_hash == b.structural_hash
        && a.content_hash != b.content_hash
        && a.identity_hash == b.identity_hash
}

// ── Helpers ──────────────────────────────────────────────────────────

fn collect_structural_hashes(node: &AstNode) -> HashSet<[u8; 32]> {
    let mut set = HashSet::new();
    set.insert(node.structural_hash);
    for child in &node.children {
        set.extend(collect_structural_hashes(child));
    }
    set
}

pub fn collect_normalised_tokens(node: &AstNode) -> Vec<String> {
    if node.children.is_empty() {
        if is_identifier_kind(&node.kind) {
            vec!["<IDENTIFIER>".to_string()]
        } else {
            vec![node.text.clone()]
        }
    } else {
        let mut tokens = Vec::new();
        for child in &node.children {
            tokens.extend(collect_normalised_tokens(child));
        }
        tokens
    }
}

pub fn collect_leaf_tokens(node: &AstNode) -> Vec<String> {
    if node.children.is_empty() {
        if !node.text.is_empty() {
            vec![node.text.clone()]
        } else {
            vec![node.kind.clone()]
        }
    } else {
        let mut tokens = Vec::new();
        for child in &node.children {
            tokens.extend(collect_leaf_tokens(child));
        }
        tokens
    }
}

/// Returns `true` if the node kind represents an identifier / name token across all 13 supported languages.
#[inline]
pub fn is_identifier_kind(kind: &str) -> bool {
    matches!(
        kind,
        "identifier"
            | "type_identifier"
            | "field_identifier"
            | "property_identifier"
            | "shorthand_field_identifier"
            | "shorthand_property_identifier"
            | "namespace_identifier"
            | "package_identifier"
            | "property_name"
            | "key"
            | "name"
            | "type_parameter"
            | "generic_type"
            | "primitive_type"
            | "scoped_identifier"
            | "scoped_type_identifier"
            | "variable_name"
            | "constant"
            | "symbol"
            | "decorator"
            | "attribute_name"
            | "class_name"
            | "method_name"
            | "function_name"
    )
}

/// Returns `true` if the node kind is a comment.
#[inline]
pub fn is_comment_or_whitespace(kind: &str) -> bool {
    matches!(
        kind,
        "comment"
            | "line_comment"
            | "block_comment"
            | "doc_comment"
            | "documentation_comment"
            | "multiline_comment"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AstNode;

    // ── Builder helpers ───────────────────────────────────────────────

    fn leaf(kind: &str, text: &str) -> AstNode {
        AstNode {
            id: 0,
            kind: kind.to_string(),
            start_byte: 0,
            end_byte: text.len(),
            start_row: 0,
            start_col: 0,
            end_row: 0,
            end_col: text.len(),
            text: text.to_string(),
            structural_hash: [0u8; 32],
            content_hash: [0u8; 32],
            context_hash: [0u8; 32],
            identity_hash: [0u8; 32],
            children: vec![],
            is_named: true,
        }
    }

    fn inner(kind: &str, children: Vec<AstNode>) -> AstNode {
        AstNode {
            id: 0,
            kind: kind.to_string(),
            start_byte: 0,
            end_byte: 0,
            start_row: 0,
            start_col: 0,
            end_row: 0,
            end_col: 0,
            text: String::new(),
            structural_hash: [0u8; 32],
            content_hash: [0u8; 32],
            context_hash: [0u8; 32],
            identity_hash: [0u8; 32],
            children,
            is_named: true,
        }
    }

    // ── Structural hash (pure tree shape) ───────────────────────────

    #[test]
    fn leaf_structural_hash_is_nonzero() {
        let mut node = leaf("identifier", "foo");
        compute_hashes(&mut node, false);
        assert_ne!(node.structural_hash, [0u8; 32]);
    }

    #[test]
    fn same_kind_leaf_same_structural_hash() {
        // structural_hash only captures kind, not text
        let mut a = leaf("identifier", "foo");
        let mut b = leaf("identifier", "bar");
        compute_hashes(&mut a, false);
        compute_hashes(&mut b, false);
        assert_eq!(a.structural_hash, b.structural_hash);
    }

    #[test]
    fn different_kind_different_structural_hash() {
        let mut a = leaf("identifier", "x");
        let mut b = leaf("type_identifier", "x");
        compute_hashes(&mut a, false);
        compute_hashes(&mut b, false);
        assert_ne!(a.structural_hash, b.structural_hash);
    }

    #[test]
    fn inner_node_structural_hash_depends_on_child_kinds() {
        let mut tree1 = inner(
            "block",
            vec![leaf("identifier", "a"), leaf("identifier", "b")],
        );
        let mut tree2 = inner(
            "block",
            vec![leaf("identifier", "x"), leaf("identifier", "y")],
        );
        compute_hashes(&mut tree1, false);
        compute_hashes(&mut tree2, false);
        // Same structure (block -> identifier, identifier) → equal
        assert_eq!(tree1.structural_hash, tree2.structural_hash);
    }

    #[test]
    fn different_child_kinds_different_structural_hash() {
        let mut tree1 = inner("block", vec![leaf("identifier", "a")]);
        let mut tree2 = inner("block", vec![leaf("integer_literal", "1")]);
        compute_hashes(&mut tree1, false);
        compute_hashes(&mut tree2, false);
        assert_ne!(tree1.structural_hash, tree2.structural_hash);
    }

    // ── Content hash (normalised tokens) ──────────────────────────────

    #[test]
    fn content_hash_is_nonzero() {
        let mut node = leaf("identifier", "foo");
        compute_hashes(&mut node, false);
        assert_ne!(node.content_hash, [0u8; 32]);
    }

    #[test]
    fn identifiers_differ_in_content_hash() {
        // content_hash preserves actual text, so different identifiers differ
        let mut a = leaf("identifier", "foo");
        let mut b = leaf("identifier", "bar");
        compute_hashes(&mut a, false);
        compute_hashes(&mut b, false);
        assert_ne!(a.content_hash, b.content_hash);
    }

    #[test]
    fn same_identifiers_same_content_hash() {
        let mut a = leaf("identifier", "foo");
        let mut b = leaf("identifier", "foo");
        compute_hashes(&mut a, false);
        compute_hashes(&mut b, false);
        assert_eq!(a.content_hash, b.content_hash);
    }

    #[test]
    fn non_identifier_content_differs() {
        let mut a = leaf("integer_literal", "1");
        let mut b = leaf("integer_literal", "2");
        compute_hashes(&mut a, false);
        compute_hashes(&mut b, false);
        assert_ne!(a.content_hash, b.content_hash);
    }

    // ── Context hash (position in tree) ───────────────────────────────

    #[test]
    fn context_hash_is_nonzero() {
        let mut node = leaf("identifier", "foo");
        compute_hashes(&mut node, false);
        assert_ne!(node.context_hash, [0u8; 32]);
    }

    #[test]
    fn children_have_different_context_hashes_from_parent() {
        let mut tree = inner("block", vec![leaf("identifier", "a")]);
        compute_hashes(&mut tree, false);
        assert_ne!(tree.context_hash, tree.children[0].context_hash);
    }

    // ── Identity hash (rename detection) ──────────────────────────────

    #[test]
    fn identifier_normalised_in_identity_hash() {
        let mut a = leaf("identifier", "foo");
        let mut b = leaf("identifier", "bar");
        compute_hashes(&mut a, false);
        compute_hashes(&mut b, false);
        assert_eq!(a.identity_hash, b.identity_hash);
    }

    #[test]
    fn non_identifier_leaf_not_normalised() {
        let mut a = leaf("integer_literal", "1");
        let mut b = leaf("integer_literal", "2");
        compute_hashes(&mut a, false);
        compute_hashes(&mut b, false);
        assert_ne!(a.identity_hash, b.identity_hash);
    }

    #[test]
    fn renamed_function_body_same_identity() {
        let mut call_a = inner(
            "call_expression",
            vec![leaf("identifier", "my_func"), leaf("integer_literal", "42")],
        );
        let mut call_b = inner(
            "call_expression",
            vec![
                leaf("identifier", "other_func"),
                leaf("integer_literal", "42"),
            ],
        );
        compute_hashes(&mut call_a, false);
        compute_hashes(&mut call_b, false);
        // Structural hash is the same (same tree shape)
        assert_eq!(call_a.structural_hash, call_b.structural_hash);
        // Identity hash is the same (identifiers normalised)
        assert_eq!(call_a.identity_hash, call_b.identity_hash);
    }

    // ── logic_only flag ───────────────────────────────────────────────

    #[test]
    fn comment_leaf_logic_only_uses_placeholder() {
        let mut a = leaf("line_comment", "// todo: remove");
        let mut b = leaf("line_comment", "// completely different");
        compute_hashes(&mut a, true);
        compute_hashes(&mut b, true);
        assert_eq!(a.content_hash, b.content_hash);
        assert_eq!(a.identity_hash, b.identity_hash);
    }

    #[test]
    fn comment_leaf_non_logic_only_differs() {
        let mut a = leaf("line_comment", "// a");
        let mut b = leaf("line_comment", "// b");
        compute_hashes(&mut a, false);
        compute_hashes(&mut b, false);
        assert_ne!(a.content_hash, b.content_hash);
    }

    // ── Children hashes propagate upward ─────────────────────────────

    #[test]
    fn parent_content_hash_changes_when_child_changes() {
        let mut unchanged = inner(
            "function_item",
            vec![leaf("identifier", "foo"), leaf("integer_literal", "1")],
        );
        let mut changed = inner(
            "function_item",
            vec![leaf("identifier", "foo"), leaf("integer_literal", "999")],
        );
        compute_hashes(&mut unchanged, false);
        compute_hashes(&mut changed, false);
        assert_ne!(unchanged.content_hash, changed.content_hash);
    }

    // ── Similarity functions ─────────────────────────────────────────

    #[test]
    fn identical_nodes_similarity_is_one() {
        let mut a = inner(
            "block",
            vec![leaf("identifier", "x"), leaf("integer_literal", "1")],
        );
        let mut b = inner(
            "block",
            vec![leaf("identifier", "x"), leaf("integer_literal", "1")],
        );
        compute_hashes(&mut a, false);
        compute_hashes(&mut b, false);
        assert!((structural_similarity(&a, &b) - 1.0).abs() < f64::EPSILON);
        assert!((token_similarity(&a, &b) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn completely_different_nodes_low_similarity() {
        let mut a = inner("block", vec![leaf("identifier", "x")]);
        let mut b = inner(
            "function_item",
            vec![
                leaf("integer_literal", "42"),
                leaf("string_literal", "hello"),
            ],
        );
        compute_hashes(&mut a, false);
        compute_hashes(&mut b, false);
        assert!(composite_similarity(&a, &b) < 0.5);
    }

    #[test]
    fn only_identifiers_changed_detects_rename() {
        // Two call expressions with different identifier names but same literal
        // structural_hash same (same tree shape), content_hash differs (different text),
        // identity_hash same (identifiers normalised to <IDENTIFIER>)
        let mut a = inner(
            "call_expression",
            vec![leaf("identifier", "foo"), leaf("integer_literal", "1")],
        );
        let mut b = inner(
            "call_expression",
            vec![leaf("identifier", "bar"), leaf("integer_literal", "1")],
        );
        compute_hashes(&mut a, false);
        compute_hashes(&mut b, false);
        assert_eq!(
            a.structural_hash, b.structural_hash,
            "structural should match"
        );
        assert_ne!(a.content_hash, b.content_hash, "content should differ");
        assert_eq!(a.identity_hash, b.identity_hash, "identity should match");
        assert!(only_identifiers_changed(&a, &b));
    }

    #[test]
    fn content_change_not_only_identifiers() {
        let mut a = inner("block", vec![leaf("integer_literal", "1")]);
        let mut b = inner("block", vec![leaf("integer_literal", "2")]);
        compute_hashes(&mut a, false);
        compute_hashes(&mut b, false);
        assert!(!only_identifiers_changed(&a, &b));
    }

    // ── Incremental hash reuse ────────────────────────────────────────

    #[test]
    fn incremental_hashes_match_full_hashes_unchanged() {
        // Build a tree and compute hashes normally
        let mut original = inner(
            "function_item",
            vec![leaf("identifier", "foo"), leaf("integer_literal", "42")],
        );
        compute_hashes(&mut original, false);

        // Build same tree, compute with incremental (no changed ranges)
        let mut incremental = inner(
            "function_item",
            vec![leaf("identifier", "foo"), leaf("integer_literal", "42")],
        );
        let empty_ranges: Vec<tree_sitter::Range> = vec![];
        let reused = compute_hashes_incremental(&mut incremental, &original, &empty_ranges, false);

        assert_eq!(incremental.structural_hash, original.structural_hash);
        assert_eq!(incremental.content_hash, original.content_hash);
        assert_eq!(incremental.identity_hash, original.identity_hash);
        assert!(reused > 0, "should reuse all nodes when nothing changed");
    }

    #[test]
    fn incremental_hashes_match_full_hashes_changed() {
        // Build old tree
        let mut old = inner(
            "block",
            vec![leaf("integer_literal", "1"), leaf("integer_literal", "2")],
        );
        old.start_byte = 0;
        old.end_byte = 10;
        old.children[0].start_byte = 0;
        old.children[0].end_byte = 5;
        old.children[1].start_byte = 5;
        old.children[1].end_byte = 10;
        compute_hashes(&mut old, false);

        // Build new tree (second child changed)
        let mut new_inc = inner(
            "block",
            vec![leaf("integer_literal", "1"), leaf("integer_literal", "99")],
        );
        new_inc.start_byte = 0;
        new_inc.end_byte = 11;
        new_inc.children[0].start_byte = 0;
        new_inc.children[0].end_byte = 5;
        new_inc.children[1].start_byte = 5;
        new_inc.children[1].end_byte = 11;

        // Changed range covers the second child only
        let changed = vec![tree_sitter::Range {
            start_byte: 5,
            end_byte: 11,
            start_point: tree_sitter::Point { row: 0, column: 5 },
            end_point: tree_sitter::Point { row: 0, column: 11 },
        }];
        compute_hashes_incremental(&mut new_inc, &old, &changed, false);

        // Build same new tree with full hashes
        let mut new_full = inner(
            "block",
            vec![leaf("integer_literal", "1"), leaf("integer_literal", "99")],
        );
        new_full.start_byte = 0;
        new_full.end_byte = 11;
        new_full.children[0].start_byte = 0;
        new_full.children[0].end_byte = 5;
        new_full.children[1].start_byte = 5;
        new_full.children[1].end_byte = 11;
        compute_hashes(&mut new_full, false);

        // All hashes should match
        assert_eq!(new_inc.structural_hash, new_full.structural_hash);
        assert_eq!(new_inc.content_hash, new_full.content_hash);
        assert_eq!(new_inc.identity_hash, new_full.identity_hash);

        // First child should have been reused (its range is outside changed)
        assert_eq!(
            new_inc.children[0].content_hash, old.children[0].content_hash,
            "unchanged child should have same content hash as old"
        );
    }

    #[test]
    fn incremental_reuse_count_is_zero_when_all_changed() {
        let mut old = leaf("identifier", "foo");
        old.start_byte = 0;
        old.end_byte = 3;
        compute_hashes(&mut old, false);

        let mut new_node = leaf("identifier", "bar");
        new_node.start_byte = 0;
        new_node.end_byte = 3;

        let changed = vec![tree_sitter::Range {
            start_byte: 0,
            end_byte: 3,
            start_point: tree_sitter::Point { row: 0, column: 0 },
            end_point: tree_sitter::Point { row: 0, column: 3 },
        }];

        let reused = compute_hashes_incremental(&mut new_node, &old, &changed, false);
        assert_eq!(reused, 0, "fully changed node should not reuse hashes");
    }

    #[test]
    fn test_simd_jaccard_histogram_units() {
        let h1 = [1, 2, 3, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let h2 = [1, 2, 3, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let sim = simd_jaccard_histogram_16(&h1, &h2);
        assert!((sim - 1.0).abs() < 1e-6);

        let h3 = [0; 16];
        let sim_zero = simd_jaccard_histogram_16(&h1, &h3);
        assert_eq!(sim_zero, 0.0);
    }

    #[test]
    fn test_token_bitset_units() {
        let tokens = vec!["fn".to_string(), "main".to_string()];
        let b = token_bitset(&tokens);
        assert_ne!(b, 0);

        let empty: Vec<String> = vec![];
        assert_eq!(token_bitset(&empty), 0);
    }

    #[test]
    fn test_token_histogram_units() {
        let tokens = vec!["apple".to_string(), "banana".to_string(), "apple".to_string()];
        let hist = token_histogram_16(&tokens);
        let total: u32 = hist.iter().map(|&x| x as u32).sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn test_is_scope_declaration_boundary_all_languages() {
        assert!(is_scope_declaration_boundary("function_item"));
        assert!(is_scope_declaration_boundary("class_declaration"));
        assert!(is_scope_declaration_boundary("method_definition"));
        assert!(is_scope_declaration_boundary("impl_item"));
        assert!(is_scope_declaration_boundary("interface_declaration"));
        assert!(!is_scope_declaration_boundary("binary_expression"));
    }

    #[test]
    fn test_is_identifier_kind_all_grammars() {
        assert!(is_identifier_kind("identifier"));
        assert!(is_identifier_kind("type_identifier"));
        assert!(is_identifier_kind("field_identifier"));
        assert!(is_identifier_kind("property_identifier"));
        assert!(is_identifier_kind("name"));
        assert!(!is_identifier_kind("string_literal"));
    }

    #[test]
    fn test_token_similarity_units() {
        let n1 = leaf("string_literal", "alpha");
        let n2 = leaf("string_literal", "alpha");
        let sim_same = token_similarity(&n1, &n2);
        assert!((sim_same - 1.0).abs() < 1e-6);

        let n3 = leaf("string_literal", "omega");
        let sim_diff = token_similarity(&n1, &n3);
        assert_eq!(sim_diff, 0.0);
    }

    #[test]
    fn test_structural_similarity_empty() {
        let n1 = leaf("identifier", "a");
        let n2 = leaf("identifier", "a");
        let sim = structural_similarity(&n1, &n2);
        assert!((sim - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_composite_similarity_range() {
        let n1 = leaf("identifier", "a");
        let n2 = leaf("identifier", "b");
        let sim = composite_similarity(&n1, &n2);
        assert!((0.0..=1.0).contains(&sim));
    }

    #[test]
    fn test_collect_leaf_tokens_single() {
        let n = leaf("identifier", "hello");
        let tokens = collect_leaf_tokens(&n);
        assert_eq!(tokens, vec!["hello"]);
    }

    #[test]
    fn test_token_bitset_deterministic() {
        let t1 = vec!["a".to_string(), "b".to_string()];
        let b1 = token_bitset(&t1);
        let b2 = token_bitset(&t1);
        assert_eq!(b1, b2);
    }
}
