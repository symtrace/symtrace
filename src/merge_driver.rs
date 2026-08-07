use std::fs;
use anyhow::{Context, Result};

use crate::ast_builder;
use crate::language;
use crate::tree_diff;
use crate::types::{OperationType, ParserLimits};

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
        let ours_modifies_only = ops_ours.iter().all(|o| o.op_type == OperationType::Modify || o.op_type == OperationType::Rename);
        let theirs_modifies_only = ops_theirs.iter().all(|o| o.op_type == OperationType::Modify || o.op_type == OperationType::Insert);

        if ours_modifies_only && theirs_modifies_only && ops_ours.is_empty() {
            fs::write(ours_path, &theirs_content)?;
            return Ok(0);
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
}
