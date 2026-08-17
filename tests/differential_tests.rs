use symtrace::ast_builder::parse_content;
use symtrace::symbol_tracking::track_cross_file_symbols;
use symtrace::tree_diff::compute_diff;
use symtrace::types::{OperationType, ParserLimits, SupportedLanguage};

fn limits() -> ParserLimits {
    ParserLimits::default()
}

// ── 1. Zero False Positive Formatting & Comment Test ─────────────────

#[test]
fn zero_false_positives_rust_formatting() {
    let unformatted = "fn calc(a:i32,b:i32)->i32{/* inline comment */a+b}";
    let formatted =
        "/// Doc comment\nfn calc(a: i32, b: i32) -> i32 {\n    // line comment\n    a + b\n}\n";

    let ast_a = parse_content(unformatted, SupportedLanguage::Rust, true, &limits()).unwrap();
    let ast_b = parse_content(formatted, SupportedLanguage::Rust, true, &limits()).unwrap();

    let ops = compute_diff(Some(&ast_a), Some(&ast_b), true);
    assert!(
        ops.is_empty(),
        "Expected 0 logic operations for formatting change, got: {:?}",
        ops
    );
}

#[test]
fn zero_false_positives_javascript_formatting() {
    let raw = "function   compute ( x,y ) { return   x*y ; }";
    let pretty = "// Prettier formatted\nfunction compute(x, y) {\n  return x * y;\n}\n";

    let ast_a = parse_content(raw, SupportedLanguage::JavaScript, true, &limits()).unwrap();
    let ast_b = parse_content(pretty, SupportedLanguage::JavaScript, true, &limits()).unwrap();

    let ops = compute_diff(Some(&ast_a), Some(&ast_b), true);
    assert!(
        ops.is_empty(),
        "Expected 0 logic operations for JS formatting, got: {:?}",
        ops
    );
}

#[test]
fn zero_false_positives_python_formatting() {
    let unformatted = "def process(data):\n    # raw comment\n    return [x*2 for x in data]";
    let formatted = "# Clean docstring\ndef process(data):\n    return [x * 2 for x in data]\n";

    let ast_a = parse_content(unformatted, SupportedLanguage::Python, true, &limits()).unwrap();
    let ast_b = parse_content(formatted, SupportedLanguage::Python, true, &limits()).unwrap();

    let ops = compute_diff(Some(&ast_a), Some(&ast_b), true);
    assert!(
        ops.is_empty(),
        "Expected 0 logic operations for Python formatting, got: {:?}",
        ops
    );
}

#[test]
fn zero_false_positives_c_formatting() {
    let raw = "int add(int a,int b){return a+b;}";
    let formatted = "/* C function */\nint add(int a, int b) {\n    return a + b;\n}\n";

    let ast_a = parse_content(raw, SupportedLanguage::C, true, &limits()).unwrap();
    let ast_b = parse_content(formatted, SupportedLanguage::C, true, &limits()).unwrap();

    let ops = compute_diff(Some(&ast_a), Some(&ast_b), true);
    assert!(
        ops.is_empty(),
        "Expected 0 logic operations for C formatting, got: {:?}",
        ops
    );
}

// ── 2. Real-World Differential Refactor Verification ─────────────────

#[test]
fn differential_refactor_detection_extract_method() {
    let old_src = r#"
fn process_order(price: f64, tax_rate: f64) -> f64 {
    let total = price + (price * tax_rate);
    total
}
"#;

    let new_src = r#"
fn calculate_tax(price: f64, tax_rate: f64) -> f64 {
    price * tax_rate
}

fn process_order(price: f64, tax_rate: f64) -> f64 {
    let total = price + calculate_tax(price, tax_rate);
    total
}
"#;

    let ast_a = parse_content(old_src, SupportedLanguage::Rust, false, &limits()).unwrap();
    let ast_b = parse_content(new_src, SupportedLanguage::Rust, false, &limits()).unwrap();

    let ops = compute_diff(Some(&ast_a), Some(&ast_b), false);
    let inserts: Vec<_> = ops
        .iter()
        .filter(|o| o.op_type == OperationType::Insert)
        .collect();
    assert!(
        !inserts.is_empty(),
        "Extracted helper method must produce an INSERT operation"
    );
}

#[test]
fn differential_cross_file_movement() {
    let old_mod_a = "pub fn shared_utility() -> &'static str { \"v1\" }";
    let new_mod_a = "";
    let new_mod_b = "pub fn shared_utility() -> &'static str { \"v1\" }";

    let ast_old_a = parse_content(old_mod_a, SupportedLanguage::Rust, false, &limits()).unwrap();
    let ast_new_a = parse_content(new_mod_a, SupportedLanguage::Rust, false, &limits()).unwrap();
    let ast_new_b = parse_content(new_mod_b, SupportedLanguage::Rust, false, &limits()).unwrap();

    let pairs = vec![
        ("src/mod_a.rs".to_string(), Some(ast_old_a), Some(ast_new_a)),
        ("src/mod_b.rs".to_string(), None, Some(ast_new_b)),
    ];

    let tracking = track_cross_file_symbols(&pairs);
    assert_eq!(tracking.cross_file_events.len(), 1);
    assert_eq!(tracking.cross_file_events[0].old_file, "src/mod_a.rs");
    assert_eq!(tracking.cross_file_events[0].new_file, "src/mod_b.rs");
}

// ── 3. v0.4.5 Specific Verification Tests ─────────────────────────────

#[test]
fn test_frequency_multiset_jaccard_token_similarity() {
    let code_a = "fn foo() { let x = 100 + 100 + 100; }";
    let code_b = "fn foo() { let x = 100 + 200 + 300; }";

    let ast_a = parse_content(code_a, SupportedLanguage::Rust, false, &limits()).unwrap();
    let ast_b = parse_content(code_b, SupportedLanguage::Rust, false, &limits()).unwrap();

    let sim = symtrace::node_identity::token_similarity(&ast_a, &ast_b);
    assert!(sim > 0.0 && sim < 1.0, "Expected multiset Jaccard score between 0.0 and 1.0, got {sim}");
}

#[test]
fn test_oversized_file_windowed_collection() {
    let code = "fn line1() {}\nfn line2() {}\nfn line3() {}\nfn line4() {}\n";
    let ast = parse_content(code, SupportedLanguage::Rust, false, &limits()).unwrap();

    let nodes = symtrace::tree_diff::collect_significant_nodes_windowed(
        &ast,
        &[],
        2_000_000, // file_size > threshold (1 MiB)
        Some(&[(0, 1)]), // changed window covers line 1 only
    );

    assert!(!nodes.is_empty());
}

#[test]
fn test_positional_displacement_penalty() {
    let old_code = "fn process(user: i32, account: i32) { save(user, account); }";
    let new_code = "fn process(account: i32, user: i32) { save(account, user); }";

    let ast_a = parse_content(old_code, SupportedLanguage::Rust, false, &limits()).unwrap();
    let ast_b = parse_content(new_code, SupportedLanguage::Rust, false, &limits()).unwrap();

    let penalty = symtrace::semantic_similarity::compute_positional_penalty(&ast_a, &ast_b);
    assert!(penalty > 0.0, "Permuted parameter sequence must produce a positional displacement penalty");
}

// ── 4. v0.5.0 Phase 1 Verification Tests ──────────────────────────────

#[test]
fn test_operator_token_mutation_sensitivity() {
    let code_assign = "fn update(x: &mut i32, y: i32) { *x = y; }";
    let code_add_assign = "fn update(x: &mut i32, y: i32) { *x += y; }";

    let ast_a = parse_content(code_assign, SupportedLanguage::Rust, false, &limits()).unwrap();
    let ast_b = parse_content(code_add_assign, SupportedLanguage::Rust, false, &limits()).unwrap();

    // Structural hashes or content hashes must differ when operator changes
    assert_ne!(ast_a.content_hash, ast_b.content_hash, "Changing = to += must mutate content hash");
    let ops = symtrace::tree_diff::compute_diff(Some(&ast_a), Some(&ast_b), false);
    assert!(!ops.is_empty(), "Operator modification must produce a diff operation");
    assert!(ops.iter().any(|op| op.is_logic_op), "Operator mutation must be tagged as is_logic_op = true");
}

#[test]
fn test_3way_merge_tree_sitter_validation() {
    let dir = std::env::temp_dir();
    let base_p = dir.join("base_v050.rs");
    let ours_p = dir.join("ours_v050.rs");
    let theirs_p = dir.join("theirs_v050.rs");

    let base_code = "fn calc() -> i32 { 0 }\nfn helper() -> i32 { 1 }\n";
    let ours_code = "fn calc() -> i32 { 42 }\nfn helper() -> i32 { 1 }\n";
    let theirs_code = "fn calc() -> i32 { 0 }\nfn helper() -> i32 { 99 }\n";

    std::fs::write(&base_p, base_code).unwrap();
    std::fs::write(&ours_p, ours_code).unwrap();
    std::fs::write(&theirs_p, theirs_code).unwrap();

    let exit_code = symtrace::merge_driver::run_merge_driver(
        base_p.to_str().unwrap(),
        ours_p.to_str().unwrap(),
        theirs_p.to_str().unwrap(),
        "math.rs",
    )
    .unwrap();

    assert_eq!(exit_code, 0, "Disjoint 3-way AST modifications must cleanly merge");
    let merged_content = std::fs::read_to_string(&ours_p).unwrap();
    assert!(merged_content.contains("42"), "Must contain ours modification");
    assert!(merged_content.contains("99"), "Must contain theirs modification");
}

#[test]
fn test_windowed_pruning_deep_hierarchy() {
    let mut large_src = String::new();
    for i in 0..2000 {
        large_src.push_str(&format!("fn func_{}() {{ let val = {}; }}\n", i, i));
    }
    let ast = parse_content(&large_src, SupportedLanguage::Rust, false, &limits()).unwrap();

    // Changed window covers only func_10 (around line 10)
    let windowed_nodes = symtrace::tree_diff::collect_significant_nodes_windowed(
        &ast,
        &[],
        2_000_000,
        Some(&[(9, 11)]),
    );

    // Only nodes overlapping lines 9-11 should be collected (pruning >99% of 2000 functions)
    assert!(!windowed_nodes.is_empty(), "Must collect nodes overlapping the window");
    assert!(windowed_nodes.len() <= 10, "Must prune all unneeded subtrees (collected {} nodes)", windowed_nodes.len());
    assert!(windowed_nodes.iter().any(|n| n.name == "func_10"));
}

// ── 5. v0.5.0 Phase 2 Micro-Commit & Adaptive Granularity Tests ────────

#[test]
fn test_fast_path_isomorphic_micro_edit() {
    let old_code = "fn start_server() { let port = 8080; println!(\"port: {}\", port); }";
    let new_code = "fn start_server() { let port = 3000; println!(\"port: {}\", port); }";

    let ast_a = parse_content(old_code, SupportedLanguage::Rust, false, &limits()).unwrap();
    let ast_b = parse_content(new_code, SupportedLanguage::Rust, false, &limits()).unwrap();

    // Structurally isomorphic: tree shapes are identical
    assert_eq!(ast_a.structural_hash, ast_b.structural_hash);
    assert_ne!(ast_a.content_hash, ast_b.content_hash);

    let ops = symtrace::tree_diff::compute_diff(Some(&ast_a), Some(&ast_b), false);
    assert!(!ops.is_empty(), "Fast-path isomorphic edit must detect modified operations");
    assert!(ops.iter().all(|op| op.op_type == symtrace::types::OperationType::Modify));
}

#[test]
fn test_micro_compact_renderer_single_line_edit() {
    let old_code = "fn run() { let status = 200; }";
    let new_code = "fn run() { let status = 404; }";

    let ast_a = parse_content(old_code, SupportedLanguage::Rust, false, &limits()).unwrap();
    let ast_b = parse_content(new_code, SupportedLanguage::Rust, false, &limits()).unwrap();

    let ops = symtrace::tree_diff::compute_diff(Some(&ast_a), Some(&ast_b), false);
    let file_diff = symtrace::types::FileDiff {
        file_path: "src/server.rs".to_string(),
        operations: ops,
        refactor_patterns: vec![],
    };

    let summary = symtrace::types::DiffSummary {
        total_files: 1,
        moves: 0,
        renames: 0,
        inserts: 0,
        deletes: 0,
        modifications: 1,
    };

    let output = symtrace::types::DiffOutput {
        repository: "micro_repo".to_string(),
        commit_a: "HEAD~1".to_string(),
        commit_b: "HEAD".to_string(),
        files: vec![file_diff],
        summary,
        cross_file_tracking: None,
        commit_classification: None,
        performance: symtrace::types::PerformanceMetrics {
            total_files_processed: 1,
            total_nodes_compared: 10,
            parse_time_ms: 0.5,
            diff_time_ms: 0.1,
            total_time_ms: 0.6,
            incremental_parses: 0,
            nodes_reused: 0,
        },
        granularity: None,
        blast_radius: None,
        contract_violations: None,
    };

    let auto_granularity = symtrace::output::determine_granularity(&output, false, false);
    assert_eq!(auto_granularity, symtrace::output::DisplayGranularity::MicroCompact);

    let rendered = symtrace::output::format_micro_cli(&output);
    assert!(rendered.contains("src/server.rs"));
    assert!(rendered.contains("[MODIFY]"));
    assert!(!rendered.contains("━━━ Performance ━━━"));
}

// ── 6. v0.5.0 Phase 3 Deep System Optimizations Tests ───────────────

#[test]
fn test_simd_jaccard_histogram_calculation() {
    let mut hist_a = [0u8; 16];
    let mut hist_b = [0u8; 16];

    for i in 0..16 {
        hist_a[i] = 10;
        hist_b[i] = 10;
    }
    // Identical histograms -> 1.0
    let sim_same = symtrace::node_identity::simd_jaccard_histogram_16(&hist_a, &hist_b);
    assert!((sim_same - 1.0).abs() < 1e-6);

    for i in 0..16 {
        hist_b[i] = 5;
    }
    // Intersection = 5*16 = 80, Union = 10*16 = 160 -> 0.5
    let sim_half = symtrace::node_identity::simd_jaccard_histogram_16(&hist_a, &hist_b);
    assert!((sim_half - 0.5).abs() < 1e-6);
}

#[test]
fn test_zero_copy_parse_bytes() {
    let code_bytes = b"fn compute(x: i32) -> i32 { x * 2 }";
    let ast = symtrace::ast_builder::parse_bytes(code_bytes, SupportedLanguage::Rust, false, &limits()).unwrap();
    assert_eq!(ast.kind, "source_file");
    assert!(!ast.children.is_empty());
}

#[test]
fn test_parallel_global_node_index_build() {
    let src1 = "fn alpha() { let a = 1; }";
    let src2 = "fn beta() { let b = 2; }";
    let src3 = "fn gamma() { let c = 3; }";

    let ast1 = parse_content(src1, SupportedLanguage::Rust, false, &limits()).unwrap();
    let ast2 = parse_content(src2, SupportedLanguage::Rust, false, &limits()).unwrap();
    let ast3 = parse_content(src3, SupportedLanguage::Rust, false, &limits()).unwrap();

    let files = [
        ("src/a.rs", &ast1),
        ("src/b.rs", &ast2),
        ("src/c.rs", &ast3),
    ];

    let global_index = symtrace::tree_diff::GlobalNodeIndex::build(&files);
    assert!(global_index.find_candidate_for_move("function_item", "alpha", "src/b.rs").is_some());
    assert!(global_index.find_candidate_for_move("function_item", "beta", "src/a.rs").is_some());
    assert!(global_index.find_candidate_for_move("function_item", "gamma", "src/a.rs").is_some());
}

// ── 7. v0.5.0 Phase 4 Contextual Semantic Intelligence Tests ────────

#[test]
fn test_cross_file_call_graph_transitive_blast_radius() {
    let service_src = "fn compute_total(price: f64, tax: f64) -> f64 { price + tax }";
    let checkout_src = "fn checkout(cart: i32) { let total = compute_total(100.0, 10.0); }";
    let api_src = "fn handle_order(req: i32) { checkout(req); }";

    let ast_service = parse_content(service_src, SupportedLanguage::Rust, false, &limits()).unwrap();
    let ast_checkout = parse_content(checkout_src, SupportedLanguage::Rust, false, &limits()).unwrap();
    let ast_api = parse_content(api_src, SupportedLanguage::Rust, false, &limits()).unwrap();

    let files = [
        ("src/service.rs", &ast_service),
        ("src/checkout.rs", &ast_checkout),
        ("src/api.rs", &ast_api),
    ];

    let call_graph = symtrace::call_graph::CallGraph::build(&files);
    let blast_reports = call_graph.compute_blast_radius(&[("compute_total", "src/service.rs")]);

    assert_eq!(blast_reports.len(), 1);
    let r = &blast_reports[0];
    assert_eq!(r.modified_symbol, "compute_total");
    // Transitive impact: checkout (depth 1) + handle_order (depth 2)
    assert_eq!(r.total_impacted_callers, 2);
    assert!(r.impacted_callers.iter().any(|c| c.caller_symbol == "checkout" && c.depth == 1));
    assert!(r.impacted_callers.iter().any(|c| c.caller_symbol == "handle_order" && c.depth == 2));
}

#[test]
fn test_data_flow_cosmetic_rename_vs_mutation() {
    let old_src = "fn process() { let i = 0; return i; }";
    let rename_src = "fn process() { let idx = 0; return idx; }";
    let mutate_src = "fn process() { let i = 0; return i + 1; }";

    let ast_old = parse_content(old_src, SupportedLanguage::Rust, false, &limits()).unwrap();
    let ast_rename = parse_content(rename_src, SupportedLanguage::Rust, false, &limits()).unwrap();
    let ast_mutate = parse_content(mutate_src, SupportedLanguage::Rust, false, &limits()).unwrap();

    let analysis_rename = symtrace::data_flow::analyze_intra_procedural_data_flow(&ast_old, &ast_rename);
    assert_eq!(analysis_rename.tag, symtrace::data_flow::DataFlowTag::CosmeticLocalRename);

    let analysis_mutate = symtrace::data_flow::analyze_intra_procedural_data_flow(&ast_old, &ast_mutate);
    assert_eq!(analysis_mutate.tag, symtrace::data_flow::DataFlowTag::DataFlowMutated);
}

#[test]
fn test_contract_violation_safety_guard_removal() {
    let old_src = "fn handle_ptr(ptr: *const u8) { if ptr != null { process(ptr); } }";
    let new_src = "fn handle_ptr(ptr: *const u8) { process(ptr); }";

    let ast_old = parse_content(old_src, SupportedLanguage::Rust, false, &limits()).unwrap();
    let ast_new = parse_content(new_src, SupportedLanguage::Rust, false, &limits()).unwrap();

    let violations = symtrace::semantic_type::detect_contract_violations("src/driver.rs", &ast_old, &ast_new);
    assert!(!violations.is_empty(), "Removing null check must trigger contract violation alert");
    assert_eq!(violations[0].rule, "REMOVED_NULL_CHECK");
    assert_eq!(violations[0].severity, "CRITICAL");
}

#[test]
fn test_type_safe_refactor_detection() {
    let old_src = "fn find_user() -> Option<User> { None }";
    let new_src = "fn find_user() -> Result<User, Error> { Err(Error::NotFound) }";

    let ast_old = parse_content(old_src, SupportedLanguage::Rust, false, &limits()).unwrap();
    let ast_new = parse_content(new_src, SupportedLanguage::Rust, false, &limits()).unwrap();

    let type_refactor = symtrace::semantic_type::detect_type_safe_refactors(&ast_old, &ast_new);
    assert!(type_refactor.is_some(), "Must detect Option -> Result type upgrade");
    assert!(type_refactor.unwrap().contains("Option to Result"));
}

// ── 8. v0.5.0 Phase 5 Output Ecosystem, Linter & Polish Tests ────────

#[test]
fn test_utf8_char_offset_alignment_with_emojis() {
    let line = "let msg = \"🚀 Launching rocket\"; // test";
    // '🚀' is 4 UTF-8 bytes but 1 character
    let rocket_byte_idx = line.find('🚀').unwrap();
    let after_rocket_byte_idx = rocket_byte_idx + '🚀'.len_utf8();

    let char_col = symtrace::ast_builder::byte_to_char_col(line, after_rocket_byte_idx);
    // Prefix 'let msg = "' is 11 chars + 1 char for emoji = 12
    assert_eq!(char_col, 12);
}

#[test]
fn test_declarative_linter_rule_severity_and_threshold() {
    let engine = symtrace::query_dsl::QueryEngine {
        rules: vec![
            symtrace::query_dsl::CustomQueryRule {
                name: "no_panic".to_string(),
                query_src: ";; @severity ERROR\n;; @message Avoid explicit panic calls\n(macro_invocation) @macro".to_string(),
                severity: symtrace::query_dsl::RuleSeverity::Error,
                message: "Avoid explicit panic calls in $file".to_string(),
            },
            symtrace::query_dsl::CustomQueryRule {
                name: "no_unwrap".to_string(),
                query_src: ";; @severity WARN\n;; @message Consider using ? instead of unwrap\n(call_expression) @call".to_string(),
                severity: symtrace::query_dsl::RuleSeverity::Warn,
                message: "Consider using ? instead of unwrap in $file".to_string(),
            },
        ],
    };

    let bad_src = "fn compute() { panic!(\"critical error\"); let x = val.unwrap(); }";
    let ast = parse_content(bad_src, SupportedLanguage::Rust, false, &limits()).unwrap();

    let files = [("src/critical.rs", &ast)];
    let result = engine.lint_files(&files, 0);

    assert_eq!(result.total_files_scanned, 1);
    assert_eq!(result.errors, 1);
    assert_eq!(result.warnings, 1);
    assert!(!result.passed, "Errors present must cause lint check to fail");

    // With max_warnings = 5, still fails because of error
    let result2 = engine.lint_files(&files, 5);
    assert!(!result2.passed);
}

// ── 9. v0.5.0 Phase 6 Quality Assurance, Multi-Language & Stress Tests ──

#[test]
fn test_differential_typescript_interface_refactor() {
    let old_ts = "interface UserConfig { host: string; port: number; }";
    let new_ts = "interface UserConfig { host: string; port: number; sslEnabled: boolean; }";

    let ast_old = parse_content(old_ts, SupportedLanguage::TypeScript, false, &limits()).unwrap();
    let ast_new = parse_content(new_ts, SupportedLanguage::TypeScript, false, &limits()).unwrap();

    let ops = compute_diff(Some(&ast_old), Some(&ast_new), false);
    assert!(!ops.is_empty(), "TypeScript interface field addition must produce operations");
}

#[test]
fn test_differential_python_async_def_mutation() {
    let old_py = "def fetch_data(url):\n    return requests.get(url)\n";
    let new_py = "async def fetch_data(url):\n    return await client.get(url)\n";

    let ast_old = parse_content(old_py, SupportedLanguage::Python, false, &limits()).unwrap();
    let ast_new = parse_content(new_py, SupportedLanguage::Python, false, &limits()).unwrap();

    let ops = compute_diff(Some(&ast_old), Some(&ast_new), false);
    assert!(!ops.is_empty(), "Python sync to async migration must produce operations");
}

#[test]
fn test_differential_go_error_handling_idiom() {
    let old_go = "package main\nfunc run() error {\n    err := doWork()\n    if err != nil {\n        return err\n    }\n    return nil\n}";
    let new_go = "package main\nfunc run() error {\n    _ = doWork()\n    return nil\n}";

    let ast_old = parse_content(old_go, SupportedLanguage::Go, false, &limits()).unwrap();
    let ast_new = parse_content(new_go, SupportedLanguage::Go, false, &limits()).unwrap();

    let ops = compute_diff(Some(&ast_old), Some(&ast_new), false);
    assert!(!ops.is_empty(), "Removing Go error check guard must produce operations");
}

#[test]
fn test_differential_java_generic_refactoring() {
    let old_java = "public class Store<T> { private T item; public T get() { return item; } }";
    let new_java = "public class Store<T extends Serializable> { private T item; public T get() { return item; } }";

    let ast_old = parse_content(old_java, SupportedLanguage::Java, false, &limits()).unwrap();
    let ast_new = parse_content(new_java, SupportedLanguage::Java, false, &limits()).unwrap();

    let ops = compute_diff(Some(&ast_old), Some(&ast_new), false);
    assert!(!ops.is_empty(), "Java generic bounds update must produce operations");
}

#[test]
fn test_differential_cpp_smart_pointer_transition() {
    let old_cpp = "class Controller { private: Worker* worker; public: void init() { worker = new Worker(); } };";
    let new_cpp = "class Controller { private: std::unique_ptr<Worker> worker; public: void init() { worker = std::make_unique<Worker>(); } };";

    let ast_old = parse_content(old_cpp, SupportedLanguage::Cpp, false, &limits()).unwrap();
    let ast_new = parse_content(new_cpp, SupportedLanguage::Cpp, false, &limits()).unwrap();

    let ops = compute_diff(Some(&ast_old), Some(&ast_new), false);
    assert!(!ops.is_empty(), "C++ raw pointer to unique_ptr must produce operations");
}

#[test]
fn test_differential_c_macro_and_struct_mutation() {
    let old_c = "typedef struct { int x; int y; } Point;\nvoid move(Point* p) { p->x += 1; }";
    let new_c = "typedef struct { int x; int y; int z; } Point;\nvoid move(Point* p) { p->x += 1; p->z += 1; }";

    let ast_old = parse_content(old_c, SupportedLanguage::C, false, &limits()).unwrap();
    let ast_new = parse_content(new_c, SupportedLanguage::C, false, &limits()).unwrap();

    let ops = compute_diff(Some(&ast_old), Some(&ast_new), false);
    assert!(!ops.is_empty(), "C struct 3D point expansion must produce operations");
}

#[test]
fn test_differential_json_ast_structural_equality() {
    let json1 = r#"{"name": "symtrace", "version": "0.5.0"}"#;
    let json2 = r#"{"name": "symtrace", "version": "0.5.1"}"#;

    let ast1 = parse_content(json1, SupportedLanguage::Json, false, &limits()).unwrap();
    let ast2 = parse_content(json2, SupportedLanguage::Json, false, &limits()).unwrap();

    assert_eq!(ast1.structural_hash, ast2.structural_hash, "JSON structural shape is invariant under string literal value mutation");
    assert_ne!(ast1.content_hash, ast2.content_hash, "JSON content hash must change when string literal value differs");
}

#[test]
fn test_differential_multi_file_pr_100_files() {
    let file_asts_old: Vec<(String, symtrace::types::AstNode)> = (0..100)
        .map(|i| {
            let path = format!("src/module_{}.rs", i);
            let code_old = format!("pub fn handle_{}(x: i32) -> i32 {{ x + {} }}\n", i, i);
            let a = parse_content(&code_old, SupportedLanguage::Rust, false, &limits()).unwrap();
            (path, a)
        })
        .collect();

    let file_asts_new: Vec<(String, symtrace::types::AstNode)> = (0..100)
        .map(|i| {
            let path = format!("src/module_{}.rs", i);
            let code_new = format!("pub fn handle_{}(x: i32) -> i32 {{ x + {} + 1 }}\n", i, i);
            let b = parse_content(&code_new, SupportedLanguage::Rust, false, &limits()).unwrap();
            (path, b)
        })
        .collect();

    let old_refs: Vec<(&str, &symtrace::types::AstNode)> = file_asts_old.iter().map(|(p, a)| (p.as_str(), a)).collect();
    let new_refs: Vec<(&str, &symtrace::types::AstNode)> = file_asts_new.iter().map(|(p, b)| (p.as_str(), b)).collect();

    let results = symtrace::tree_diff::compute_multi_file_diff(&old_refs, &new_refs, false);
    assert_eq!(results.len(), 100, "100-file PR simulation must process all 100 files");
}

#[test]
fn test_differential_syntax_error_recovery_ast() {
    let broken_code = "fn broken() { let x = 42; // missing brace";
    let valid_code = "fn broken() { let x = 42; }";

    let ast_broken = parse_content(broken_code, SupportedLanguage::Rust, false, &limits()).unwrap();
    let ast_valid = parse_content(valid_code, SupportedLanguage::Rust, false, &limits()).unwrap();

    assert!(!ast_broken.children.is_empty(), "Tree-sitter error recovery must build non-empty AST for broken syntax");
    let ops = compute_diff(Some(&ast_broken), Some(&ast_valid), false);
    let _ = ops;
}

#[test]
fn test_differential_llm_prompt_density_and_content() {
    let diff_output = symtrace::types::DiffOutput {
        repository: "symtrace".to_string(),
        commit_a: "v0.4.5".to_string(),
        commit_b: "v0.5.0".to_string(),
        files: vec![
            symtrace::types::FileDiff {
                file_path: "src/engine.rs".to_string(),
                operations: vec![symtrace::types::OperationRecord {
                    op_type: symtrace::types::OperationType::Modify,
                    entity_type: symtrace::types::EntityType::Function,
                    old_location: Some("L10".to_string()),
                    new_location: Some("L10".to_string()),
                    details: "fn optimize() speedups".to_string(),
                    similarity: None,
                    is_logic_op: true,
                }],
                refactor_patterns: vec![],
            }
        ],
        summary: symtrace::types::DiffSummary {
            total_files: 1,
            moves: 0,
            renames: 0,
            inserts: 0,
            deletes: 0,
            modifications: 1,
        },
        cross_file_tracking: None,
        commit_classification: None,
        performance: symtrace::types::PerformanceMetrics {
            total_files_processed: 1,
            total_nodes_compared: 20,
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

    let prompt = symtrace::output::format_prompt(&diff_output);
    assert!(prompt.contains("=== symtrace SEMANTIC CONTEXT ==="));
    assert!(prompt.contains("src/engine.rs at L10"));
}

#[test]
fn test_differential_blast_radius_multilevel_dag() {
    let code_a = "pub fn leaf_node() {}";
    let code_b = "pub fn mid_node() { leaf_node(); }";
    let code_c = "pub fn top_node() { mid_node(); }";

    let ast_a = parse_content(code_a, SupportedLanguage::Rust, false, &limits()).unwrap();
    let ast_b = parse_content(code_b, SupportedLanguage::Rust, false, &limits()).unwrap();
    let ast_c = parse_content(code_c, SupportedLanguage::Rust, false, &limits()).unwrap();

    let files = [
        ("src/a.rs", &ast_a),
        ("src/b.rs", &ast_b),
        ("src/c.rs", &ast_c),
    ];

    let graph = symtrace::call_graph::CallGraph::build(&files);
    let reports = graph.compute_blast_radius(&[("leaf_node", "src/a.rs")]);

    assert!(!reports.is_empty());
    assert!(reports[0].total_impacted_callers >= 2, "Transitive callers mid_node and top_node must be detected");
}

#[test]
fn test_differential_contract_violation_mutex_lock_removal() {
    let old_code = "fn write_data() { state.lock(); send(); }";
    let new_code = "fn write_data() { send(); }";

    let ast_old = parse_content(old_code, SupportedLanguage::Rust, false, &limits()).unwrap();
    let ast_new = parse_content(new_code, SupportedLanguage::Rust, false, &limits()).unwrap();

    let violations = symtrace::semantic_type::detect_contract_violations("src/thread.rs", &ast_old, &ast_new);
    assert!(!violations.is_empty(), "Stripping .lock() must trigger contract violation");
    assert_eq!(violations[0].rule, "STRIPPED_LOCK_GUARD");
}

#[test]
fn test_differential_cas_cache_roundtrip_warm_hit() {
    let cache = symtrace::ast_cache::AstCache::new(None);
    let key = symtrace::ast_cache::DiffCacheKey {
        old_blob_hash: "1111111111111111111111111111111111111111".to_string(),
        new_blob_hash: "2222222222222222222222222222222222222222".to_string(),
        logic_only: true,
        limits_hash: 42u64,
    };

    let sample_diff = symtrace::types::FileDiff {
        file_path: "src/cached.rs".to_string(),
        operations: vec![symtrace::types::OperationRecord {
            op_type: symtrace::types::OperationType::Insert,
            entity_type: symtrace::types::EntityType::Function,
            old_location: None,
            new_location: Some("L1".to_string()),
            details: "fn new_cached()".to_string(),
            similarity: None,
            is_logic_op: true,
        }],
        refactor_patterns: vec![],
    };

    cache.put_diff(&key, sample_diff.clone());
    let hit = cache.get_diff(&key);
    assert!(hit.is_some(), "Warm CAS cache must return Some on precomputed key");
    assert_eq!(hit.unwrap().operations.len(), 1);
}
