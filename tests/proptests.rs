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

    /// Property 5: Token bitset subset invariant — bitset of subset is a bitwise submask of superset.
    #[test]
    fn prop_token_bitset_subset(
        tokens1 in proptest::collection::vec("[a-z]{3,6}", 1..5),
        tokens2 in proptest::collection::vec("[a-z]{3,6}", 1..5),
    ) {
        let mut combined = tokens1.clone();
        combined.extend(tokens2);

        let b1 = symtrace::node_identity::token_bitset(&tokens1);
        let b_comb = symtrace::node_identity::token_bitset(&combined);

        prop_assert_eq!(b1 & b_comb, b1, "Bitset of subset must be contained in combined bitset");
    }

    /// Property 6: Token histogram sum — frequency counts sum to number of tokens.
    #[test]
    fn prop_token_histogram_sum(
        tokens in proptest::collection::vec("[a-z]{2,5}", 1..10),
    ) {
        let hist = symtrace::node_identity::token_histogram_16(&tokens);
        let total_count: u32 = hist.iter().map(|&c| c as u32).sum();
        prop_assert_eq!(total_count, tokens.len() as u32);
    }

    /// Property 7: SIMD Jaccard histogram bounds — score is strictly within [0.0, 1.0].
    #[test]
    fn prop_simd_jaccard_histogram_bounds(
        h1 in proptest::array::uniform16(0u8..10),
        h2 in proptest::array::uniform16(0u8..10),
    ) {
        let score = symtrace::node_identity::simd_jaccard_histogram_16(&h1, &h2);
        prop_assert!((0.0..=1.0).contains(&score), "Jaccard score {} must be in [0.0, 1.0]", score);
    }

    /// Property 8: SIMD Jaccard self-symmetry — identical non-empty histograms yield 1.0.
    #[test]
    fn prop_simd_jaccard_self_symmetry(
        h in proptest::array::uniform16(1u8..10),
    ) {
        let score = symtrace::node_identity::simd_jaccard_histogram_16(&h, &h);
        prop_assert!((score - 1.0).abs() < 1e-6, "Self-similarity must be 1.0, got {}", score);
    }

    /// Property 9: UTF-8 byte-to-char offset monotonicity — char column <= byte offset.
    #[test]
    fn prop_byte_to_char_monotonicity(
        s in "[a-zA-Z0-9 🚀🦀]{1,30}",
        idx in 0usize..30,
    ) {
        let char_col = symtrace::ast_builder::byte_to_char_col(&s, idx);
        prop_assert!(char_col <= idx, "Char col {} must be <= byte idx {}", char_col, idx);
    }

    /// Property 10: UTF-8 ASCII equality — for ASCII strings, char col == byte offset.
    #[test]
    fn prop_byte_to_char_ascii(
        s in "[a-zA-Z0-9 ]{1,30}",
        idx in 0usize..30,
    ) {
        let char_col = symtrace::ast_builder::byte_to_char_col(&s, idx);
        let expected = idx.min(s.len());
        prop_assert_eq!(char_col, expected);
    }

    /// Property 11: Content hash change — modifying a numeric constant changes content hash.
    #[test]
    fn prop_content_hash_changes_on_literal_change(
        val1 in 0i32..100,
        val2 in 101i32..200,
    ) {
        let code1 = format!("fn val() -> i32 {{ {} }}", val1);
        let code2 = format!("fn val() -> i32 {{ {} }}", val2);
        let limits = ParserLimits::default();

        if let (Ok(ast1), Ok(ast2)) = (
            parse_content(&code1, SupportedLanguage::Rust, false, &limits),
            parse_content(&code2, SupportedLanguage::Rust, false, &limits),
        ) {
            prop_assert_ne!(ast1.content_hash, ast2.content_hash);
        }
    }

    /// Property 12: Diff cache key digest stability — identical inputs produce identical BLAKE3 digest.
    #[test]
    fn prop_diff_cache_key_digest_stability(
        oid_a in "[a-f0-9]{40}",
        oid_b in "[a-f0-9]{40}",
        logic in proptest::bool::ANY,
    ) {
        let key1 = symtrace::ast_cache::DiffCacheKey {
            old_blob_hash: oid_a.clone(),
            new_blob_hash: oid_b.clone(),
            logic_only: logic,
            limits_hash: 0u64,
        };
        let key2 = symtrace::ast_cache::DiffCacheKey {
            old_blob_hash: oid_a,
            new_blob_hash: oid_b,
            logic_only: logic,
            limits_hash: 0u64,
        };
        prop_assert_eq!(key1.digest(), key2.digest());
    }

    /// Property 13: Diff cache key digest uniqueness — different OIDs produce distinct digests.
    #[test]
    fn prop_diff_cache_key_digest_uniqueness(
        oid_a in "[a-f0-9]{40}",
        oid_b1 in "[0-4]{40}",
        oid_b2 in "[5-9]{40}",
    ) {
        let key1 = symtrace::ast_cache::DiffCacheKey {
            old_blob_hash: oid_a.clone(),
            new_blob_hash: oid_b1,
            logic_only: false,
            limits_hash: 0u64,
        };
        let key2 = symtrace::ast_cache::DiffCacheKey {
            old_blob_hash: oid_a,
            new_blob_hash: oid_b2,
            logic_only: false,
            limits_hash: 0u64,
        };
        prop_assert_ne!(key1.digest(), key2.digest());
    }

    /// Property 14: Data-flow self-analysis invariant — comparing a function to itself is always Unchanged.
    #[test]
    fn prop_data_flow_self_analysis_unchanged(
        name in "[a-z]{3,8}",
        val in 1i32..100,
    ) {
        let code = format!("fn {}() -> i32 {{ let x = {}; return x; }}", name, val);
        let limits = ParserLimits::default();
        if let Ok(ast) = parse_content(&code, SupportedLanguage::Rust, false, &limits) {
            let analysis = symtrace::data_flow::analyze_intra_procedural_data_flow(&ast, &ast);
            prop_assert_eq!(analysis.tag, symtrace::data_flow::DataFlowTag::Unchanged);
        }
    }

    /// Property 15: Call graph blast radius boundedness — blast radius never exceeds total nodes.
    #[test]
    fn prop_call_graph_blast_radius_bounded_by_nodes(
        sym in "[a-z]{3,6}",
    ) {
        let code1 = format!("fn {}() {{ }}", sym);
        let limits = ParserLimits::default();
        if let Ok(ast) = parse_content(&code1, SupportedLanguage::Rust, false, &limits) {
            let files = [("src/lib.rs", &ast)];
            let graph = symtrace::call_graph::CallGraph::build(&files);
            let reports = graph.compute_blast_radius(&[(&sym, "src/lib.rs")]);
            for r in reports {
                prop_assert!(r.total_impacted_callers <= graph.nodes.len());
            }
        }
    }

    /// Property 16: AST cache memory put/get consistency.
    #[test]
    fn prop_ast_cache_put_get_consistency(
        path in "[a-z]{3,8}\\.rs",
    ) {
        let cache = symtrace::ast_cache::AstCache::new(None);
        let key = symtrace::ast_cache::DiffCacheKey {
            old_blob_hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string(),
            new_blob_hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string(),
            logic_only: false,
            limits_hash: 1u64,
        };
        let diff = symtrace::types::FileDiff {
            file_path: path.clone(),
            operations: vec![],
            refactor_patterns: vec![],
        };

        cache.put_diff(&key, diff.clone());
        let retrieved = cache.get_diff(&key);
        prop_assert!(retrieved.is_some());
        prop_assert_eq!(retrieved.unwrap().file_path, path);
    }

    /// Property 17: 3-way merge of identical inputs produces identical source without markers.
    #[test]
    fn prop_3way_merge_identical_clean(
        val in 1i32..100,
    ) {
        let code = format!("fn compute() -> i32 {{ {} }}\n", val);
        let dir = std::env::temp_dir().join(format!("symtrace_prop_merge_{}", val));
        let _ = std::fs::create_dir_all(&dir);

        let base_f = dir.join("base.rs");
        let ours_f = dir.join("ours.rs");
        let theirs_f = dir.join("theirs.rs");

        std::fs::write(&base_f, &code).unwrap();
        std::fs::write(&ours_f, &code).unwrap();
        std::fs::write(&theirs_f, &code).unwrap();

        let base_str = base_f.to_string_lossy().to_string();
        let ours_str = ours_f.to_string_lossy().to_string();
        let theirs_str = theirs_f.to_string_lossy().to_string();

        let exit_code = symtrace::merge_driver::run_merge_driver(&base_str, &ours_str, &theirs_str, "file.rs").unwrap();
        prop_assert_eq!(exit_code, 0);

        let merged = std::fs::read_to_string(&ours_f).unwrap();
        prop_assert_eq!(merged, code);

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Property 18: Adaptive granularity controller — large op count always promotes to Standard/FullStructural.
    #[test]
    fn prop_granularity_promotion_monotonicity(
        op_count in 4usize..20,
    ) {
        let mut ops = Vec::new();
        for i in 0..op_count {
            ops.push(symtrace::types::OperationRecord {
                op_type: symtrace::types::OperationType::Modify,
                entity_type: symtrace::types::EntityType::Function,
                old_location: Some(format!("L{}", i)),
                new_location: Some(format!("L{}", i + 1)),
                details: format!("mod {}", i),
                similarity: None,
                is_logic_op: true,
            });
        }
        let output = symtrace::types::DiffOutput {
            repository: "repo".to_string(),
            commit_a: "a".to_string(),
            commit_b: "b".to_string(),
            files: vec![symtrace::types::FileDiff {
                file_path: "src/main.rs".to_string(),
                operations: ops,
                refactor_patterns: vec![],
            }],
            summary: symtrace::types::DiffSummary {
                total_files: 1,
                moves: 0,
                renames: 0,
                inserts: 0,
                deletes: 0,
                modifications: op_count,
            },
            cross_file_tracking: None,
            commit_classification: None,
            performance: symtrace::types::PerformanceMetrics {
                total_files_processed: 1,
                total_nodes_compared: 10,
                parse_time_ms: 0.1,
                diff_time_ms: 0.1,
                total_time_ms: 0.2,
                incremental_parses: 0,
                nodes_reused: 0,
            },
            granularity: None,
            blast_radius: None,
            contract_violations: None,
        };

        let gran = symtrace::output::determine_granularity(&output, false, false);
        prop_assert_eq!(gran, symtrace::output::DisplayGranularity::Standard);
    }

    /// Property 19: Positional penalty bounds in [0.0, 1.0].
    #[test]
    fn prop_multiset_positional_penalty_bounds(
        val1 in 1i32..50,
        val2 in 51i32..100,
    ) {
        let code1 = format!("fn foo() {{ let a = {}; let b = {}; }}", val1, val2);
        let code2 = format!("fn foo() {{ let b = {}; let a = {}; }}", val2, val1);
        let limits = ParserLimits::default();
        if let (Ok(ast1), Ok(ast2)) = (
            parse_content(&code1, SupportedLanguage::Rust, false, &limits),
            parse_content(&code2, SupportedLanguage::Rust, false, &limits),
        ) {
            let penalty = symtrace::semantic_similarity::compute_positional_penalty(&ast1, &ast2);
            prop_assert!((0.0..=1.0).contains(&penalty));
        }
    }

    /// Property 20: Type-safe refactor detector never panics on arbitrary function pairs.
    #[test]
    fn prop_type_safe_refactor_fuzz(
        ret1 in "[A-Z][a-z]{2,6}",
        ret2 in "[A-Z][a-z]{2,6}",
    ) {
        let code1 = format!("fn test() -> {} {{ }}", ret1);
        let code2 = format!("fn test() -> {} {{ }}", ret2);
        let limits = ParserLimits::default();
        if let (Ok(ast1), Ok(ast2)) = (
            parse_content(&code1, SupportedLanguage::Rust, false, &limits),
            parse_content(&code2, SupportedLanguage::Rust, false, &limits),
        ) {
            let _ = symtrace::semantic_type::detect_type_safe_refactors(&ast1, &ast2);
        }
    }
}
