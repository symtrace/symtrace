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
