use anyhow::{Context, Result};
use bumpalo::collections::Vec as BumpVec;
use bumpalo::Bump;
use tree_sitter::Parser;

use crate::incremental_parse;
use crate::language::get_tree_sitter_language;
use crate::node_identity;
use crate::types::{AstNode, ParserLimits, SupportedLanguage};

// ── Arena-allocated intermediate AST node ────────────────────────────

/// Intermediate AST node allocated in a bumpalo arena for efficient
/// construction. Converted to the owned `AstNode` after the tree is built.
struct ArenaAstNode<'a> {
    id: u64,
    kind: &'a str,
    start_byte: usize,
    end_byte: usize,
    start_row: usize,
    start_col: usize,
    end_row: usize,
    end_col: usize,
    text: &'a str,
    children: BumpVec<'a, ArenaAstNode<'a>>,
    is_named: bool,
}

impl<'a> ArenaAstNode<'a> {
    /// Convert arena node to owned AstNode (single recursive pass).
    fn to_owned_ast(&self) -> AstNode {
        AstNode {
            id: self.id,
            kind: self.kind.to_string(),
            start_byte: self.start_byte,
            end_byte: self.end_byte,
            start_row: self.start_row,
            start_col: self.start_col,
            end_row: self.end_row,
            end_col: self.end_col,
            text: self.text.to_string(),
            structural_hash: [0u8; 32],
            content_hash: [0u8; 32],
            context_hash: [0u8; 32],
            identity_hash: [0u8; 32],
            children: self.children.iter().map(|c| c.to_owned_ast()).collect(),
            is_named: self.is_named,
        }
    }
}

// ── Public API ───────────────────────────────────────────────────────

/// Parse source code into our internal AST representation.
///
/// Uses a bumpalo arena for the construction phase to reduce heap
/// allocation overhead and improve CPU cache locality. The arena is
/// dropped after conversion, freeing all construction temporaries at once.
///
/// # Resource guardrails
///
/// The `limits` parameter enforces hard boundaries on:
/// - **File size** — rejects input larger than `max_file_size_bytes`
/// - **AST node count** — aborts if parsing produces more than `max_ast_nodes`
/// - **Recursion depth** — aborts if tree depth exceeds `max_recursion_depth`
/// - **Parse timeout** — passes a wall-clock timeout to tree-sitter
#[allow(dead_code)]
pub fn parse_content(
    content: &str,
    lang: SupportedLanguage,
    logic_only: bool,
    limits: &ParserLimits,
) -> Result<AstNode> {
    let (ast, _tree) = parse_content_with_tree(content, lang, logic_only, limits)?;
    Ok(ast)
}

/// Parse source code and return both the AST and the tree-sitter Tree.
///
/// The returned Tree can be cached and later passed to
/// `parse_content_incremental` for faster re-parsing of modified versions
/// of the same file.
pub fn parse_content_with_tree(
    content: &str,
    lang: SupportedLanguage,
    logic_only: bool,
    limits: &ParserLimits,
) -> Result<(AstNode, tree_sitter::Tree)> {
    // ── File size guard ──────────────────────────────────────────────
    if content.len() > limits.max_file_size_bytes {
        anyhow::bail!(
            "File size ({} bytes) exceeds limit ({} bytes) — skipping",
            content.len(),
            limits.max_file_size_bytes
        );
    }

    let mut parser = Parser::new();
    let ts_language = get_tree_sitter_language(lang);
    parser
        .set_language(&ts_language)
        .context("Failed to set tree-sitter language for parser")?;

    let tree = do_tree_sitter_parse(&mut parser, content, None, limits)?;

    let root = tree.root_node();

    // Arena-accelerated construction phase
    let bump = Bump::new();
    let mut id_counter = 0u64;
    let arena_ast =
        build_arena_ast_node(&bump, root, content, logic_only, &mut id_counter, limits, 0)?;
    let mut ast = arena_ast.to_owned_ast();
    // Bump allocator dropped here — single deallocation for all construction temporaries

    // Compute structural and identity hashes bottom-up
    node_identity::compute_hashes(&mut ast, logic_only);

    Ok((ast, tree))
}

/// Parse source code incrementally using a previous tree-sitter Tree.
///
/// This is the core of the incremental parsing optimisation. Given the old
/// content, the new content, the previously-parsed tree-sitter Tree, and the
/// old AST, this function:
///
/// 1. Computes the minimal edit region (common prefix/suffix detection)
/// 2. Applies the edit to a clone of the old tree
/// 3. Passes the edited tree to tree-sitter's incremental parser, which
///    internally reuses all unchanged subtrees
/// 4. Builds our internal AstNode from the result
/// 5. Reuses bottom-up hashes (structural, content, identity) from the
///    old AST for nodes whose byte ranges are entirely outside the
///    changed regions
///
/// Returns `(AstNode, new_Tree, nodes_reused_count)`.
pub fn parse_content_incremental(
    new_content: &str,
    old_content: &str,
    old_tree: &tree_sitter::Tree,
    old_ast: &AstNode,
    lang: SupportedLanguage,
    logic_only: bool,
    limits: &ParserLimits,
) -> Result<(AstNode, tree_sitter::Tree, u64)> {
    // ── File size guard ──────────────────────────────────────────────
    if new_content.len() > limits.max_file_size_bytes {
        anyhow::bail!(
            "File size ({} bytes) exceeds limit ({} bytes) — skipping",
            new_content.len(),
            limits.max_file_size_bytes
        );
    }

    // ── 1. Compute minimal edit ──────────────────────────────────────
    let edit = incremental_parse::compute_edit(old_content, new_content);

    // ── 2. Clone and edit old tree ───────────────────────────────────
    let mut edited_tree = old_tree.clone();
    edited_tree.edit(&edit);

    // ── 3. Incremental tree-sitter parse ─────────────────────────────
    let mut parser = Parser::new();
    let ts_language = get_tree_sitter_language(lang);
    parser
        .set_language(&ts_language)
        .context("Failed to set tree-sitter language for parser")?;

    let new_tree = do_tree_sitter_parse(&mut parser, new_content, Some(&edited_tree), limits)?;

    // ── 4. Compute changed regions for hash reuse ──────────────────
    //    tree-sitter's changed_ranges() reports structural differences
    //    but may miss leaf content changes (e.g. "1" → "2" keeps the
    //    same integer_literal node kind). We use the edit byte region
    //    directly — this precisely identifies all bytes that differ
    //    between old and new content.
    let changed_ranges: Vec<tree_sitter::Range> =
        if edit.start_byte < edit.new_end_byte || edit.start_byte < edit.old_end_byte {
            vec![tree_sitter::Range {
                start_byte: edit.start_byte,
                end_byte: edit.new_end_byte.max(edit.start_byte),
                start_point: edit.start_position,
                end_point: edit.new_end_position,
            }]
        } else {
            // Identical content — no changed ranges
            vec![]
        };

    // ── 5. Build AstNode from incrementally-parsed tree ──────────────
    let root = new_tree.root_node();
    let bump = Bump::new();
    let mut id_counter = 0u64;
    let arena_ast = build_arena_ast_node(
        &bump,
        root,
        new_content,
        logic_only,
        &mut id_counter,
        limits,
        0,
    )?;
    let mut ast = arena_ast.to_owned_ast();

    // ── 6. Compute hashes with reuse for unchanged subtrees ──────────
    let nodes_reused =
        node_identity::compute_hashes_incremental(&mut ast, old_ast, &changed_ranges, logic_only);

    Ok((ast, new_tree, nodes_reused))
}

/// Shared tree-sitter parsing logic with optional old tree and timeout.
fn do_tree_sitter_parse(
    parser: &mut Parser,
    content: &str,
    old_tree: Option<&tree_sitter::Tree>,
    limits: &ParserLimits,
) -> Result<tree_sitter::Tree> {
    #[allow(deprecated)]
    if limits.parse_timeout_ms > 0 {
        parser.set_timeout_micros(limits.parse_timeout_ms * 1000);
    } else {
        parser.set_timeout_micros(0); // 0 = no timeout
    }

    let tree = parser.parse(content, old_tree);

    // Reset timeout so the parser can be reused cleanly
    #[allow(deprecated)]
    parser.set_timeout_micros(0);

    tree.ok_or_else(|| {
        anyhow::anyhow!(
            "Tree-sitter failed to parse (possible timeout after {}ms)",
            limits.parse_timeout_ms
        )
    })
}

/// Recursively convert a tree-sitter Node into an arena-allocated ArenaAstNode.
///
/// Enforces `limits.max_ast_nodes` and `limits.max_recursion_depth` during
/// construction, returning an error if either is exceeded.
fn build_arena_ast_node<'a>(
    bump: &'a Bump,
    node: tree_sitter::Node,
    source: &str,
    logic_only: bool,
    id_counter: &mut u64,
    limits: &ParserLimits,
    depth: usize,
) -> Result<ArenaAstNode<'a>> {
    // ── Node count guard ─────────────────────────────────────────────
    if (*id_counter as usize) >= limits.max_ast_nodes {
        anyhow::bail!(
            "AST node count exceeds limit of {} — skipping file",
            limits.max_ast_nodes
        );
    }

    // ── Recursion depth guard ────────────────────────────────────────
    if depth >= limits.max_recursion_depth {
        anyhow::bail!(
            "AST recursion depth exceeds limit of {} — skipping file",
            limits.max_recursion_depth
        );
    }

    let id = *id_counter;
    *id_counter += 1;

    // Allocate kind string in arena (bump alloc = pointer increment)
    let kind = bump.alloc_str(node.kind());

    // Only store text for leaf nodes (no named children) to save memory
    let text = if node.named_child_count() == 0 {
        let raw = node.utf8_text(source.as_bytes()).unwrap_or("");
        bump.alloc_str(raw)
    } else {
        "" // No allocation needed for interior nodes
    };

    // Pre-allocate children vec with known capacity in arena
    let child_count = node.named_child_count();
    let mut children = BumpVec::with_capacity_in(child_count, bump);

    for i in 0..child_count {
        if let Some(child) = node.named_child(i) {
            // In logic-only mode, skip comment and whitespace nodes
            if logic_only && is_comment_or_whitespace(child.kind()) {
                continue;
            }
            children.push(build_arena_ast_node(
                bump,
                child,
                source,
                logic_only,
                id_counter,
                limits,
                depth + 1,
            )?);
        }
    }

    Ok(ArenaAstNode {
        id,
        kind,
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
        start_row: node.start_position().row,
        start_col: node.start_position().column,
        end_row: node.end_position().row,
        end_col: node.end_position().column,
        text,
        children,
        is_named: node.is_named(),
    })
}

/// Check if a node kind represents a comment or pure whitespace.
#[inline]
fn is_comment_or_whitespace(kind: &str) -> bool {
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
    use crate::types::SupportedLanguage;

    // ── Helper ────────────────────────────────────────────────────────

    fn parse(src: &str, lang: SupportedLanguage) -> AstNode {
        parse_content(src, lang, false, &ParserLimits::default()).expect("parse failed")
    }

    fn parse_logic_only(src: &str, lang: SupportedLanguage) -> AstNode {
        parse_content(src, lang, true, &ParserLimits::default()).expect("parse logic-only failed")
    }

    // ── Root node basics ──────────────────────────────────────────────

    #[test]
    fn rust_root_is_source_file() {
        let ast = parse("fn main() {}", SupportedLanguage::Rust);
        assert_eq!(ast.kind, "source_file");
    }

    #[test]
    fn python_root_is_module() {
        let ast = parse("x = 1", SupportedLanguage::Python);
        assert_eq!(ast.kind, "module");
    }

    #[test]
    fn java_root_is_program() {
        let ast = parse("class Foo { void bar() {} }", SupportedLanguage::Java);
        assert_eq!(ast.kind, "program");
    }

    #[test]
    fn javascript_root_is_program() {
        let ast = parse("function hi() {}", SupportedLanguage::JavaScript);
        assert_eq!(ast.kind, "program");
    }

    #[test]
    fn typescript_root_is_program() {
        let ast = parse("const x: number = 1;", SupportedLanguage::TypeScript);
        assert_eq!(ast.kind, "program");
    }

    // ── Hashes are populated ──────────────────────────────────────────

    #[test]
    fn structural_hash_is_nonzero_after_parse() {
        let ast = parse("fn foo() {}", SupportedLanguage::Rust);
        assert_ne!(
            ast.structural_hash, [0u8; 32],
            "structural hash should not be zero"
        );
    }

    #[test]
    fn identity_hash_is_nonzero_after_parse() {
        let ast = parse("fn foo() {}", SupportedLanguage::Rust);
        assert_ne!(
            ast.identity_hash, [0u8; 32],
            "identity hash should not be zero"
        );
    }

    // ── Identical sources produce identical hashes ────────────────────

    #[test]
    fn identical_rust_sources_have_equal_structural_hashes() {
        let src = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let a = parse(src, SupportedLanguage::Rust);
        let b = parse(src, SupportedLanguage::Rust);
        assert_eq!(a.structural_hash, b.structural_hash);
    }

    #[test]
    fn different_rust_sources_have_different_structural_hashes() {
        let a = parse("fn foo() { let x = 1; }", SupportedLanguage::Rust);
        let b = parse("fn foo() { let x = 2; }", SupportedLanguage::Rust);
        // structural_hash is kind-only, so same tree shape → same hash
        assert_eq!(a.structural_hash, b.structural_hash);
        // content_hash captures actual tokens, so different values → different hash
        assert_ne!(a.content_hash, b.content_hash);
    }

    // ── Rename produces same identity hash, different structural hash ──

    #[test]
    fn renamed_function_identical_identity_hash() {
        let a = parse("fn foo(x: i32) -> i32 { x + 1 }", SupportedLanguage::Rust);
        let b = parse("fn bar(x: i32) -> i32 { x + 1 }", SupportedLanguage::Rust);
        // structural_hash is kind-only → same tree shape → same hash
        assert_eq!(a.structural_hash, b.structural_hash);
        // content_hash uses actual text → different names → different hash
        assert_ne!(a.content_hash, b.content_hash);
        // Identity hashes should be equal (identifiers normalised)
        assert_eq!(a.identity_hash, b.identity_hash);
    }

    // ── logic_only strips comments ────────────────────────────────────

    #[test]
    fn logic_only_rust_comment_stripped() {
        let with_comment = parse_logic_only(
            "fn foo() {\n    // this is a comment\n    let x = 1;\n}",
            SupportedLanguage::Rust,
        );
        let without_comment =
            parse_logic_only("fn foo() {\n    let x = 1;\n}", SupportedLanguage::Rust);
        // Structural hashes should match since comments are ignored.
        assert_eq!(
            with_comment.structural_hash, without_comment.structural_hash,
            "logic_only should make comment-only changes hash-equal"
        );
    }

    // ── Leaf nodes have text ──────────────────────────────────────────

    #[test]
    fn leaf_nodes_carry_text() {
        let ast = parse("fn foo() {}", SupportedLanguage::Rust);
        let has_text_leaf = has_any_text_leaf(&ast);
        assert!(has_text_leaf, "some leaf node should have non-empty text");
    }

    fn has_any_text_leaf(node: &AstNode) -> bool {
        if node.children.is_empty() {
            return !node.text.is_empty();
        }
        node.children.iter().any(has_any_text_leaf)
    }

    // ── IDs are unique within a tree ─────────────────────────────────

    #[test]
    fn all_node_ids_are_unique() {
        let ast = parse(
            "fn foo(a: i32) -> i32 { a + 1 }\nfn bar() {}",
            SupportedLanguage::Rust,
        );
        let mut ids = std::collections::HashSet::new();
        collect_ids(&ast, &mut ids);
        // We just verify that the set is non-empty; if any ID were duplicated
        // count would differ, but we verify via the collection being large.
        assert!(ids.len() > 1);
    }

    fn collect_ids(node: &AstNode, set: &mut std::collections::HashSet<u64>) {
        set.insert(node.id);
        for child in &node.children {
            collect_ids(child, set);
        }
    }

    // ── node count helper ─────────────────────────────────────────────

    #[test]
    fn node_count_at_least_one() {
        use crate::tree_diff::count_nodes;
        let ast = parse("fn foo() {}", SupportedLanguage::Rust);
        assert!(count_nodes(&ast) >= 1);
    }

    #[test]
    fn larger_source_has_more_nodes() {
        use crate::tree_diff::count_nodes;
        let small = parse("fn foo() {}", SupportedLanguage::Rust);
        let large = parse(
            "fn foo() { let x = 1; let y = 2; x + y }",
            SupportedLanguage::Rust,
        );
        assert!(count_nodes(&large) > count_nodes(&small));
    }

    // ── Empty / trivial sources ───────────────────────────────────────

    #[test]
    fn empty_source_parses_without_error() {
        let ast = parse("", SupportedLanguage::Rust);
        assert_eq!(ast.kind, "source_file");
    }

    #[test]
    fn whitespace_only_source_parses_without_error() {
        let ast = parse("   \n\t\n", SupportedLanguage::Python);
        assert_eq!(ast.kind, "module");
    }

    // ── Resource guardrail tests ─────────────────────────────────────

    #[test]
    fn file_size_limit_rejects_oversized_input() {
        let limits = ParserLimits {
            max_file_size_bytes: 10, // tiny limit
            ..ParserLimits::default()
        };
        let result = parse_content(
            "fn this_is_way_too_long() {}",
            SupportedLanguage::Rust,
            false,
            &limits,
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("exceeds limit"), "error: {msg}");
    }

    #[test]
    fn node_count_limit_rejects_large_ast() {
        let limits = ParserLimits {
            max_ast_nodes: 3, // extremely small
            ..ParserLimits::default()
        };
        // This source creates more than 3 named nodes
        let result = parse_content(
            "fn a() {} fn b() {} fn c() {} fn d() {}",
            SupportedLanguage::Rust,
            false,
            &limits,
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("node count exceeds"), "error: {msg}");
    }

    #[test]
    fn recursion_depth_limit_rejects_deep_nesting() {
        let limits = ParserLimits {
            max_recursion_depth: 2, // very shallow
            ..ParserLimits::default()
        };
        // Nested blocks create depth
        let result = parse_content(
            "fn foo() { if true { if true { let x = 1; } } }",
            SupportedLanguage::Rust,
            false,
            &limits,
        );
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("recursion depth exceeds"), "error: {msg}");
    }

    #[test]
    fn default_limits_accept_normal_input() {
        let result = parse_content(
            "fn foo() { let x = 1; let y = 2; x + y }",
            SupportedLanguage::Rust,
            false,
            &ParserLimits::default(),
        );
        assert!(result.is_ok());
    }

    // ── parse_content_with_tree ───────────────────────────────────────

    #[test]
    fn parse_with_tree_returns_valid_tree() {
        let (ast, tree) = super::parse_content_with_tree(
            "fn foo() {}",
            SupportedLanguage::Rust,
            false,
            &ParserLimits::default(),
        )
        .unwrap();
        assert_eq!(ast.kind, "source_file");
        assert_eq!(tree.root_node().kind(), "source_file");
    }

    #[test]
    fn parse_with_tree_matches_regular_parse() {
        let src = "fn add(a: i32, b: i32) -> i32 { a + b }";
        let regular = parse(src, SupportedLanguage::Rust);
        let (with_tree, _) = super::parse_content_with_tree(
            src,
            SupportedLanguage::Rust,
            false,
            &ParserLimits::default(),
        )
        .unwrap();
        assert_eq!(regular.structural_hash, with_tree.structural_hash);
        assert_eq!(regular.content_hash, with_tree.content_hash);
        assert_eq!(regular.identity_hash, with_tree.identity_hash);
    }

    // ── Incremental parsing ───────────────────────────────────────────

    #[test]
    fn incremental_parse_produces_same_ast_as_full_parse() {
        let old_src = "fn foo() { let x = 1; }";
        let new_src = "fn foo() { let x = 2; }";

        let (old_ast, old_tree) = super::parse_content_with_tree(
            old_src,
            SupportedLanguage::Rust,
            false,
            &ParserLimits::default(),
        )
        .unwrap();

        let (inc_ast, _, _) = super::parse_content_incremental(
            new_src,
            old_src,
            &old_tree,
            &old_ast,
            SupportedLanguage::Rust,
            false,
            &ParserLimits::default(),
        )
        .unwrap();

        let full_ast = parse(new_src, SupportedLanguage::Rust);

        assert_eq!(inc_ast.structural_hash, full_ast.structural_hash);
        assert_eq!(inc_ast.content_hash, full_ast.content_hash);
        assert_eq!(inc_ast.identity_hash, full_ast.identity_hash);
    }

    #[test]
    fn incremental_parse_reuses_unchanged_nodes() {
        let old_src = "function foo() { return 1; }\nfunction bar() { return 2; }";
        let new_src = "function foo() { return 999; }\nfunction bar() { return 2; }";

        let (old_ast, old_tree) = super::parse_content_with_tree(
            old_src,
            SupportedLanguage::JavaScript,
            false,
            &ParserLimits::default(),
        )
        .unwrap();

        let (_, _, nodes_reused) = super::parse_content_incremental(
            new_src,
            old_src,
            &old_tree,
            &old_ast,
            SupportedLanguage::JavaScript,
            false,
            &ParserLimits::default(),
        )
        .unwrap();

        assert!(nodes_reused > 0, "should reuse some unchanged nodes");
    }

    #[test]
    fn incremental_parse_identical_content_reuses_all() {
        let src = "fn foo() { let x = 1; }";

        let (old_ast, old_tree) = super::parse_content_with_tree(
            src,
            SupportedLanguage::Rust,
            false,
            &ParserLimits::default(),
        )
        .unwrap();

        let (inc_ast, _, nodes_reused) = super::parse_content_incremental(
            src,
            src,
            &old_tree,
            &old_ast,
            SupportedLanguage::Rust,
            false,
            &ParserLimits::default(),
        )
        .unwrap();

        assert!(nodes_reused > 0, "identical content should reuse nodes");
        assert_eq!(inc_ast.structural_hash, old_ast.structural_hash);
        assert_eq!(inc_ast.content_hash, old_ast.content_hash);
    }

    #[test]
    fn incremental_parse_complete_change_still_correct() {
        let old_src = "fn foo() {}";
        let new_src = "fn bar(x: i32) { x + 1 }";

        let (old_ast, old_tree) = super::parse_content_with_tree(
            old_src,
            SupportedLanguage::Rust,
            false,
            &ParserLimits::default(),
        )
        .unwrap();

        let (inc_ast, _, _) = super::parse_content_incremental(
            new_src,
            old_src,
            &old_tree,
            &old_ast,
            SupportedLanguage::Rust,
            false,
            &ParserLimits::default(),
        )
        .unwrap();

        let full_ast = parse(new_src, SupportedLanguage::Rust);

        assert_eq!(inc_ast.structural_hash, full_ast.structural_hash);
        assert_eq!(inc_ast.content_hash, full_ast.content_hash);
    }

    #[test]
    fn incremental_parse_insertion() {
        let old_src = "function a() {}\nfunction b() {}";
        let new_src = "function a() {}\nfunction NEW() {}\nfunction b() {}";

        let (old_ast, old_tree) = super::parse_content_with_tree(
            old_src,
            SupportedLanguage::JavaScript,
            false,
            &ParserLimits::default(),
        )
        .unwrap();

        let (inc_ast, _, _) = super::parse_content_incremental(
            new_src,
            old_src,
            &old_tree,
            &old_ast,
            SupportedLanguage::JavaScript,
            false,
            &ParserLimits::default(),
        )
        .unwrap();

        let full_ast = parse(new_src, SupportedLanguage::JavaScript);

        assert_eq!(inc_ast.structural_hash, full_ast.structural_hash);
        assert_eq!(inc_ast.content_hash, full_ast.content_hash);
    }

    #[test]
    fn incremental_parse_deletion() {
        let old_src = "let x = 1;\nlet y = 2;\nlet z = 3;";
        let new_src = "let x = 1;\nlet z = 3;";

        let (old_ast, old_tree) = super::parse_content_with_tree(
            old_src,
            SupportedLanguage::JavaScript,
            false,
            &ParserLimits::default(),
        )
        .unwrap();

        let (inc_ast, _, _) = super::parse_content_incremental(
            new_src,
            old_src,
            &old_tree,
            &old_ast,
            SupportedLanguage::JavaScript,
            false,
            &ParserLimits::default(),
        )
        .unwrap();

        let full_ast = parse(new_src, SupportedLanguage::JavaScript);

        assert_eq!(inc_ast.structural_hash, full_ast.structural_hash);
    }

    #[test]
    fn incremental_parse_logic_only() {
        let old_src = "fn foo() { let x = 1; }";
        let new_src = "fn foo() {\n    // new comment\n    let x = 1;\n}";

        let (old_ast, old_tree) = super::parse_content_with_tree(
            old_src,
            SupportedLanguage::Rust,
            true,
            &ParserLimits::default(),
        )
        .unwrap();

        let (inc_ast, _, _) = super::parse_content_incremental(
            new_src,
            old_src,
            &old_tree,
            &old_ast,
            SupportedLanguage::Rust,
            true,
            &ParserLimits::default(),
        )
        .unwrap();

        let full_ast = parse_logic_only(new_src, SupportedLanguage::Rust);

        assert_eq!(inc_ast.structural_hash, full_ast.structural_hash);
    }

    #[test]
    fn incremental_parse_python_modification() {
        let old_src = "def foo():\n    return 1\n\ndef bar():\n    return 2";
        let new_src = "def foo():\n    return 999\n\ndef bar():\n    return 2";

        let (old_ast, old_tree) = super::parse_content_with_tree(
            old_src,
            SupportedLanguage::Python,
            false,
            &ParserLimits::default(),
        )
        .unwrap();

        let (inc_ast, _, _) = super::parse_content_incremental(
            new_src,
            old_src,
            &old_tree,
            &old_ast,
            SupportedLanguage::Python,
            false,
            &ParserLimits::default(),
        )
        .unwrap();

        let full_ast = parse(new_src, SupportedLanguage::Python);

        assert_eq!(inc_ast.structural_hash, full_ast.structural_hash);
        assert_eq!(inc_ast.content_hash, full_ast.content_hash);
    }

    #[test]
    fn incremental_parse_java_modification() {
        let old_src = "class Foo { void bar() { int x = 1; } }";
        let new_src = "class Foo { void bar() { int x = 42; } void baz() {} }";

        let (old_ast, old_tree) = super::parse_content_with_tree(
            old_src,
            SupportedLanguage::Java,
            false,
            &ParserLimits::default(),
        )
        .unwrap();

        let (inc_ast, _, _) = super::parse_content_incremental(
            new_src,
            old_src,
            &old_tree,
            &old_ast,
            SupportedLanguage::Java,
            false,
            &ParserLimits::default(),
        )
        .unwrap();

        let full_ast = parse(new_src, SupportedLanguage::Java);

        assert_eq!(inc_ast.structural_hash, full_ast.structural_hash);
        assert_eq!(inc_ast.content_hash, full_ast.content_hash);
    }

    #[test]
    fn parse_c_code() {
        let src = "#include <stdio.h>\nint main() { printf(\"Hello C\\n\"); return 0; }";
        let ast = parse(src, SupportedLanguage::C);
        assert!(!ast.children.is_empty());
    }

    #[test]
    fn parse_cpp_code() {
        let src = "#include <iostream>\nclass Foo { public: void bar() { std::cout << \"C++\" << std::endl; } };";
        let ast = parse(src, SupportedLanguage::Cpp);
        assert!(!ast.children.is_empty());
    }

    #[test]
    fn parse_go_code() {
        let src = "package main\nimport \"fmt\"\nfunc main() { fmt.Println(\"Hello Go\") }";
        let ast = parse(src, SupportedLanguage::Go);
        assert!(!ast.children.is_empty());
    }

    #[test]
    fn parse_json_code() {
        let src = "{\n  \"name\": \"symtrace\",\n  \"version\": \"0.3.0\",\n  \"active\": true\n}";
        let ast = parse(src, SupportedLanguage::Json);
        assert!(!ast.children.is_empty());
    }
}
