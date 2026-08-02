use proptest::prelude::*;
use symtrace::ast_builder::parse_content;
use symtrace::tree_diff::compute_diff;
use symtrace::types::{ParserLimits, SupportedLanguage};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    /// Property 1: Self-diff symmetry — compute_diff(A, A) must yield 0 operations.
    #[test]
    fn prop_diff_self_symmetry(
        val in 0i32..1000,
        name in "[a-z]{3,8}",
    ) {
        let code = format!("fn {}() -> i32 {{ {} }}", name, val);
        let limits = ParserLimits::default();
        if let Ok(ast) = parse_content(&code, SupportedLanguage::Rust, false, &limits) {
            let ops = compute_diff(Some(&ast), Some(&ast), false);
            prop_assert!(ops.is_empty(), "compute_diff(A, A) should yield 0 ops, got {:?}", ops);
        }
    }

    /// Property 2: Determinism — compute_diff(A, B) yields identical results across repeated invocations.
    #[test]
    fn prop_diff_determinism(
        val_a in 0i32..100,
        val_b in 100i32..200,
        name in "[a-z]{3,6}",
    ) {
        let code_a = format!("fn {}() -> i32 {{ {} }}", name, val_a);
        let code_b = format!("fn {}() -> i32 {{ {} }}", name, val_b);
        let limits = ParserLimits::default();

        if let (Ok(ast_a), Ok(ast_b)) = (
            parse_content(&code_a, SupportedLanguage::Rust, false, &limits),
            parse_content(&code_b, SupportedLanguage::Rust, false, &limits),
        ) {
            let run1 = compute_diff(Some(&ast_a), Some(&ast_b), false);
            let run2 = compute_diff(Some(&ast_a), Some(&ast_b), false);
            let run3 = compute_diff(Some(&ast_a), Some(&ast_b), false);

            prop_assert_eq!(&run1, &run2, "run1 and run2 diff outputs must be identical");
            prop_assert_eq!(&run2, &run3, "run2 and run3 diff outputs must be identical");
        }
    }

    /// Property 3: Structural hash invariance — renaming a function identifier leaves the structural hash unchanged.
    #[test]
    fn prop_structural_hash_rename_invariant(
        name1 in "[a-z]{4,8}",
        name2 in "[A-Z]{4,8}",
    ) {
        let code1 = format!("fn {}() {{ let x = 42; }}", name1);
        let code2 = format!("fn {}() {{ let x = 42; }}", name2);
        let limits = ParserLimits::default();

        if let (Ok(ast1), Ok(ast2)) = (
            parse_content(&code1, SupportedLanguage::Rust, false, &limits),
            parse_content(&code2, SupportedLanguage::Rust, false, &limits),
        ) {
            prop_assert_eq!(
                ast1.structural_hash,
                ast2.structural_hash,
                "structural hash must be invariant under identifier rename"
            );
        }
    }

    /// Property 4: Arbitrary string parser resilience — parse_content never panics on arbitrary string inputs.
    #[test]
    fn prop_parser_resilience(
        s in "\\PC*",
    ) {
        let limits = ParserLimits {
            max_file_size_bytes: 10_000,
            max_ast_nodes: 1_000,
            max_recursion_depth: 64,
            parse_timeout_ms: 100,
        };

        for lang in [
            SupportedLanguage::Rust,
            SupportedLanguage::JavaScript,
            SupportedLanguage::Python,
            SupportedLanguage::C,
            SupportedLanguage::Json,
        ] {
            let _ = parse_content(&s, lang, false, &limits);
        }
    }
}
