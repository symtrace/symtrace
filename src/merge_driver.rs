use std::fs;
use anyhow::{Context, Result};

use crate::ast_builder;
use crate::language;
use crate::tree_diff;
use crate::types::{OperationRecord, ParserLimits};

/// Execute native 3-way AST semantic merge driver (`git merge-driver %O %A %B %P`).
///
/// Parameters:
/// - `base_path`: Base commit temp file (%O)
/// - `ours_path`: Ours commit temp file (%A)
/// - `theirs_path`: Theirs commit temp file (%B)
/// - `display_path`: Original file path (%P)
pub fn run_merge_driver(
    base_path: &str,
    ours_path: &str,
    theirs_path: &str,
    display_path: &str,
) -> Result<i32> {
    let base_content = fs::read_to_string(base_path).unwrap_or_default();
    let ours_content = fs::read_to_string(ours_path).unwrap_or_default();
    let theirs_content = fs::read_to_string(theirs_path).unwrap_or_default();

    // 1. Identical content check
    if ours_content == theirs_content {
        fs::write(ours_path, &ours_content)?;
        return Ok(0); // Clean merge
    }

    if base_content == ours_content {
        fs::write(ours_path, &theirs_content)?;
        return Ok(0); // Clean merge
    }

    if base_content == theirs_content {
        fs::write(ours_path, &ours_content)?;
        return Ok(0); // Clean merge
    }

    // 2. Parse ASTs for 3-way AST diffing
    let limits = ParserLimits::default();
    let lang = language::detect_language(display_path);

    if let Some(l) = lang {
        let base_ast = ast_builder::parse_content(&base_content, l, false, &limits).ok();
        let ours_ast = ast_builder::parse_content(&ours_content, l, false, &limits).ok();
        let theirs_ast = ast_builder::parse_content(&theirs_content, l, false, &limits).ok();

        let ops_ours = tree_diff::compute_diff(base_ast.as_ref(), ours_ast.as_ref(), false);
        let ops_theirs = tree_diff::compute_diff(base_ast.as_ref(), theirs_ast.as_ref(), false);

        // Check if changes operate on distinct AST entities (non-conflicting AST merge)
        let ours_entities: std::collections::HashSet<&str> = ops_ours.iter().map(|o| o.details.as_str()).collect();
        let theirs_entities: std::collections::HashSet<&str> = ops_theirs.iter().map(|o| o.details.as_str()).collect();

        // Disjoint AST entity check: if ours and theirs modify completely independent AST nodes
        if ours_entities.intersection(&theirs_entities).next().is_none() {
            if ops_ours.is_empty() {
                fs::write(ours_path, &theirs_content)?;
                return Ok(0);
            } else if ops_theirs.is_empty() {
                fs::write(ours_path, &ours_content)?;
                return Ok(0);
            } else if let Some(merged_code) = combine_disjoint_ast_sources(
                &ours_content,
                &theirs_content,
                ours_ast.as_ref(),
                theirs_ast.as_ref(),
                &ops_theirs,
                l,
                &limits,
            ) {
                fs::write(ours_path, merged_code)?;
                return Ok(0);
            }
        }
    }

    // 3. True logic conflict — write camelCase conflict header annotations
    let mut conflict_buf = String::new();
    conflict_buf.push_str(&format!("<<<<<<< Ours: {}\n", display_path));
    conflict_buf.push_str(&ours_content);
    if !ours_content.ends_with('\n') {
        conflict_buf.push('\n');
    }
    conflict_buf.push_str("=======\n");
    conflict_buf.push_str(&theirs_content);
    if !theirs_content.ends_with('\n') {
        conflict_buf.push('\n');
    }
    conflict_buf.push_str(&format!(">>>>>>> Theirs: {}\n", display_path));

    fs::write(ours_path, conflict_buf).context("Failed to write 3-way merge conflict output")?;
    Ok(1) // Conflict status code
}

/// Recursively find a named entity node by its AST kind and identifier name.
fn find_named_entity<'a>(node: &'a crate::types::AstNode, kind: &str, name: &str) -> Option<&'a crate::types::AstNode> {
    if node.kind == kind && tree_diff::extract_name(node) == name {
        return Some(node);
    }
    for child in &node.children {
        if let Some(found) = find_named_entity(child, kind, name) {
            return Some(found);
        }
    }
    None
}

/// Recursively verify whether the AST contains any Tree-Sitter error or missing nodes.
fn has_ast_errors(node: &crate::types::AstNode) -> bool {
    if node.kind == "ERROR" || node.kind == "MISSING" || node.kind.contains("error") {
        return true;
    }
    node.children.iter().any(has_ast_errors)
}

/// Extract (kind, name) from operation details formatted as `<kind> '<name>' ...`.
fn parse_op_details(details: &str) -> Option<(&str, &str)> {
    let mut parts = details.splitn(2, '\'');
    let kind = parts.next()?.trim();
    let rest = parts.next()?;
    let name = rest.split('\'').next()?;
    Some((kind, name))
}

struct TextReplacement {
    start_byte: usize,
    end_byte: usize,
    new_text: String,
}

/// Combine non-overlapping AST modifications from ours and theirs using AST scope splicing
/// and validate the candidate merge with Tree-sitter re-parsing.
fn combine_disjoint_ast_sources(
    ours_src: &str,
    theirs_src: &str,
    ours_ast: Option<&crate::types::AstNode>,
    theirs_ast: Option<&crate::types::AstNode>,
    ops_theirs: &[OperationRecord],
    lang: crate::types::SupportedLanguage,
    limits: &ParserLimits,
) -> Option<String> {
    let ours_root = ours_ast?;
    let theirs_root = theirs_ast?;

    let mut replacements: Vec<TextReplacement> = Vec::new();

    for op in ops_theirs {
        match op.op_type {
            crate::types::OperationType::Modify => {
                if let Some((kind, name)) = parse_op_details(&op.details) {
                    if let (Some(n_ours), Some(n_theirs)) = (
                        find_named_entity(ours_root, kind, name),
                        find_named_entity(theirs_root, kind, name),
                    ) {
                        if n_theirs.end_byte <= theirs_src.len() && n_ours.end_byte <= ours_src.len() {
                            let theirs_slice = &theirs_src[n_theirs.start_byte..n_theirs.end_byte];
                            replacements.push(TextReplacement {
                                start_byte: n_ours.start_byte,
                                end_byte: n_ours.end_byte,
                                new_text: theirs_slice.to_string(),
                            });
                        }
                    }
                }
            }
            crate::types::OperationType::Delete => {
                if let Some((kind, name)) = parse_op_details(&op.details) {
                    if let Some(n_ours) = find_named_entity(ours_root, kind, name) {
                        if n_ours.end_byte <= ours_src.len() {
                            replacements.push(TextReplacement {
                                start_byte: n_ours.start_byte,
                                end_byte: n_ours.end_byte,
                                new_text: String::new(),
                            });
                        }
                    }
                }
            }
            crate::types::OperationType::Insert => {
                if let Some((kind, name)) = parse_op_details(&op.details) {
                    if let Some(n_theirs) = find_named_entity(theirs_root, kind, name) {
                        if n_theirs.end_byte <= theirs_src.len() {
                            let theirs_slice = &theirs_src[n_theirs.start_byte..n_theirs.end_byte];
                            let insertion_point = ours_src.len();
                            replacements.push(TextReplacement {
                                start_byte: insertion_point,
                                end_byte: insertion_point,
                                new_text: format!("\n{}", theirs_slice),
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if replacements.is_empty() {
        return None;
    }

    // Sort replacements in descending order of start_byte so byte offsets remain stable
    replacements.sort_by(|a, b| b.start_byte.cmp(&a.start_byte));

    let mut merged = ours_src.to_string();
    for repl in replacements {
        if repl.start_byte <= merged.len() && repl.end_byte <= merged.len() && repl.start_byte <= repl.end_byte {
            merged.replace_range(repl.start_byte..repl.end_byte, &repl.new_text);
        } else {
            return None;
        }
    }

    // Tree-sitter Validation Re-parse
    let candidate_ast = ast_builder::parse_content(&merged, lang, false, limits).ok()?;
    if has_ast_errors(&candidate_ast) {
        return None;
    }

    Some(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn run_merge_driver_identical_clean_merge() {
        let dir = std::env::temp_dir();
        let base_p = dir.join("base_ident.rs");
        let ours_p = dir.join("ours_ident.rs");
        let theirs_p = dir.join("theirs_ident.rs");

        fs::write(&base_p, "fn main() {}").unwrap();
        fs::write(&ours_p, "fn main() {}").unwrap();
        fs::write(&theirs_p, "fn main() {}").unwrap();

        let code = run_merge_driver(
            base_p.to_str().unwrap(),
            ours_p.to_str().unwrap(),
            theirs_p.to_str().unwrap(),
            "main.rs",
        )
        .unwrap();

        assert_eq!(code, 0);
    }

    #[test]
    fn run_merge_driver_base_equals_ours_updates_to_theirs() {
        let dir = std::env::temp_dir();
        let base_p = dir.join("base_b_eq_o.rs");
        let ours_p = dir.join("ours_b_eq_o.rs");
        let theirs_p = dir.join("theirs_b_eq_o.rs");

        fs::write(&base_p, "fn main() {}").unwrap();
        fs::write(&ours_p, "fn main() {}").unwrap();
        fs::write(&theirs_p, "fn main() { println!(\"hello\"); }").unwrap();

        let code = run_merge_driver(
            base_p.to_str().unwrap(),
            ours_p.to_str().unwrap(),
            theirs_p.to_str().unwrap(),
            "main.rs",
        )
        .unwrap();

        assert_eq!(code, 0);
        let result_content = fs::read_to_string(&ours_p).unwrap();
        assert_eq!(result_content, "fn main() { println!(\"hello\"); }");
    }

    #[test]
    fn run_merge_driver_base_equals_theirs_keeps_ours() {
        let dir = std::env::temp_dir();
        let base_p = dir.join("base_b_eq_t.rs");
        let ours_p = dir.join("ours_b_eq_t.rs");
        let theirs_p = dir.join("theirs_b_eq_t.rs");

        fs::write(&base_p, "fn main() {}").unwrap();
        fs::write(&ours_p, "fn main() { let x = 42; }").unwrap();
        fs::write(&theirs_p, "fn main() {}").unwrap();

        let code = run_merge_driver(
            base_p.to_str().unwrap(),
            ours_p.to_str().unwrap(),
            theirs_p.to_str().unwrap(),
            "main.rs",
        )
        .unwrap();

        assert_eq!(code, 0);
        let result_content = fs::read_to_string(&ours_p).unwrap();
        assert_eq!(result_content, "fn main() { let x = 42; }");
    }

    #[test]
    fn run_merge_driver_conflict_writes_headers() {
        let dir = std::env::temp_dir();
        let base_p = dir.join("base_conf.rs");
        let ours_p = dir.join("ours_conf.rs");
        let theirs_p = dir.join("theirs_conf.rs");

        fs::write(&base_p, "fn main() { let val = 1; }").unwrap();
        fs::write(&ours_p, "fn main() { let val = 2; }").unwrap();
        fs::write(&theirs_p, "fn main() { let val = 3; }").unwrap();

        let code = run_merge_driver(
            base_p.to_str().unwrap(),
            ours_p.to_str().unwrap(),
            theirs_p.to_str().unwrap(),
            "src/main.rs",
        )
        .unwrap();

        assert_eq!(code, 1);
        let conflict_content = fs::read_to_string(&ours_p).unwrap();
        assert!(conflict_content.contains("<<<<<<< Ours: src/main.rs"));
        assert!(conflict_content.contains("======="));
        assert!(conflict_content.contains(">>>>>>> Theirs: src/main.rs"));
    }

    #[test]
    fn run_merge_driver_disjoint_ast_modifications() {
        let dir = std::env::temp_dir();
        let base_p = dir.join("base_disj.rs");
        let ours_p = dir.join("ours_disj.rs");
        let theirs_p = dir.join("theirs_disj.rs");

        fs::write(&base_p, "fn foo() {}\nfn bar() {}").unwrap();
        fs::write(&ours_p, "fn foo() { println!(\"ours\"); }\nfn bar() {}").unwrap();
        fs::write(&theirs_p, "fn foo() {}\nfn bar() { println!(\"theirs\"); }").unwrap();

        let code = run_merge_driver(
            base_p.to_str().unwrap(),
            ours_p.to_str().unwrap(),
            theirs_p.to_str().unwrap(),
            "main.rs",
        )
        .unwrap();

        assert_eq!(code, 0);
    }
}
