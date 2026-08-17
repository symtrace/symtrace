//! Semantic Similarity Scoring
//!
//! Computes a composite similarity score between two AST nodes using:
//!
//! | Metric                | Weight |
//! |-----------------------|--------|
//! | structure_similarity  |  0.50  |
//! | token_similarity      |  0.30  |
//! | complexity_weight     |  0.20  |
//!
//! Output:
//! * `similarity_percent` — 0–100
//! * `change_intensity`   — low / medium / high

use crate::node_identity;
use crate::types::{AstNode, ChangeIntensity, SimilarityScore};

// ── Composite score weights ──────────────────────────────────────────

const STRUCTURE_WEIGHT: f64 = 0.5;
const TOKEN_WEIGHT: f64 = 0.3;
const COMPLEXITY_WEIGHT: f64 = 0.2;

// ── Public API ───────────────────────────────────────────────────────

/// Produce a full [`SimilarityScore`] comparing two AST subtrees.
pub fn compute_similarity(old: &AstNode, new: &AstNode) -> SimilarityScore {
    let structure_sim = node_identity::structural_similarity(old, new);
    let token_sim = node_identity::token_similarity(old, new);

    let old_count = count_nodes(old) as i64;
    let new_count = count_nodes(new) as i64;
    let node_count_delta = (new_count - old_count).abs();

    let old_cc = cyclomatic_complexity(old) as i64;
    let new_cc = cyclomatic_complexity(new) as i64;
    let cyclomatic_delta = new_cc - old_cc;

    let control_flow_changed = detect_control_flow_change(old, new);

    // Complexity factor: 1.0 when no complexity change, scaled down
    // proportionally to the magnitude of complexity change.
    let complexity_factor = if old_cc + new_cc == 0 {
        1.0
    } else {
        1.0 - (cyclomatic_delta.unsigned_abs() as f64 / (old_cc + new_cc) as f64).min(1.0)
    };

    let penalty = compute_positional_penalty(old, new);

    let composite = (structure_sim * STRUCTURE_WEIGHT
        + token_sim * TOKEN_WEIGHT
        + complexity_factor * COMPLEXITY_WEIGHT
        - penalty).clamp(0.0, 1.0);

    let similarity_percent = (composite * 100.0).clamp(0.0, 100.0);

    let change_intensity = classify_intensity(similarity_percent);

    SimilarityScore {
        structure_similarity: structure_sim,
        token_similarity: token_sim,
        node_count_delta,
        cyclomatic_delta,
        control_flow_changed,
        similarity_percent,
        change_intensity,
    }
}

use std::collections::{HashMap, VecDeque};

/// Compute a positional sequence displacement penalty when leaf tokens/parameters are permuted.
/// Uses FIFO queues per token to ensure O(N) linear time and avoid duplicate token positional distortion.
pub fn compute_positional_penalty(old: &AstNode, new: &AstNode) -> f64 {
    let tokens_a = node_identity::collect_leaf_tokens(old);
    let tokens_b = node_identity::collect_leaf_tokens(new);
    if tokens_a.is_empty() || tokens_b.is_empty() || tokens_a.len() != tokens_b.len() {
        return 0.0;
    }

    let mut b_indices: HashMap<&str, VecDeque<usize>> = HashMap::new();
    for (j, t_b) in tokens_b.iter().enumerate() {
        b_indices.entry(t_b.as_str()).or_default().push_back(j);
    }

    let mut total_disp = 0usize;
    for (i, t_a) in tokens_a.iter().enumerate() {
        if let Some(queue) = b_indices.get_mut(t_a.as_str()) {
            if let Some(j) = queue.pop_front() {
                total_disp += (i as isize - j as isize).unsigned_abs();
            }
        }
    }
    let max_disp = tokens_a.len() * tokens_a.len();
    if max_disp == 0 {
        return 0.0;
    }
    0.20 * (total_disp as f64 / max_disp as f64)
}

// ── Intensity classification ─────────────────────────────────────────

fn classify_intensity(similarity_percent: f64) -> ChangeIntensity {
    if similarity_percent >= 80.0 {
        ChangeIntensity::Low
    } else if similarity_percent >= 50.0 {
        ChangeIntensity::Medium
    } else {
        ChangeIntensity::High
    }
}

// ── Cyclomatic complexity ────────────────────────────────────────────

/// Approximate cyclomatic complexity by counting decision points.
fn cyclomatic_complexity(node: &AstNode) -> u32 {
    let decision = if is_decision_point(&node.kind) { 1 } else { 0 };
    decision + node.children.iter().map(cyclomatic_complexity).sum::<u32>()
}

fn is_decision_point(kind: &str) -> bool {
    matches!(
        kind,
        "if_expression"
            | "if_statement"
            | "else_clause"
            | "elif_clause"
            | "for_expression"
            | "for_statement"
            | "for_in_statement"
            | "while_expression"
            | "while_statement"
            | "do_statement"
            | "match_expression"
            | "match_arm"
            | "switch_statement"
            | "switch_case"
            | "case_clause"
            | "catch_clause"
            | "ternary_expression"
            | "conditional_expression"
            | "try_statement"
            | "try_expression"
            | "binary_expression" // && / ||  counted later if needed
            | "boolean_operator"
            | "logical_and"
            | "logical_or"
    )
}

// ── Control-flow change detection ────────────────────────────────────

fn detect_control_flow_change(old: &AstNode, new: &AstNode) -> bool {
    let old_kinds = collect_control_flow_kinds(old);
    let new_kinds = collect_control_flow_kinds(new);
    old_kinds != new_kinds
}

fn collect_control_flow_kinds(node: &AstNode) -> Vec<String> {
    let mut kinds = Vec::new();
    if is_control_flow_kind(&node.kind) {
        kinds.push(node.kind.clone());
    }
    for child in &node.children {
        kinds.extend(collect_control_flow_kinds(child));
    }
    kinds
}

fn is_control_flow_kind(kind: &str) -> bool {
    matches!(
        kind,
        "if_expression"
            | "if_statement"
            | "for_expression"
            | "for_statement"
            | "for_in_statement"
            | "while_expression"
            | "while_statement"
            | "do_statement"
            | "match_expression"
            | "switch_statement"
            | "try_statement"
            | "try_expression"
            | "return_statement"
            | "break_statement"
            | "continue_statement"
    )
}

// ── Helpers ──────────────────────────────────────────────────────────

fn count_nodes(node: &AstNode) -> u64 {
    1 + node.children.iter().map(count_nodes).sum::<u64>()
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
    fn identical_code_full_similarity() {
        let src = "fn foo() -> i32 { 42 }";
        let a = parse(src, SupportedLanguage::Rust);
        let b = parse(src, SupportedLanguage::Rust);
        let score = compute_similarity(&a, &b);
        assert!(
            score.similarity_percent >= 99.0,
            "identical code should be ~100%, got {:.1}%",
            score.similarity_percent
        );
        assert_eq!(score.change_intensity, ChangeIntensity::Low);
        assert!(!score.control_flow_changed);
        assert_eq!(score.cyclomatic_delta, 0);
    }

    #[test]
    fn small_change_high_similarity() {
        let a = parse("fn compute() -> i32 { 1 + 1 }", SupportedLanguage::Rust);
        let b = parse("fn compute() -> i32 { 2 + 2 }", SupportedLanguage::Rust);
        let score = compute_similarity(&a, &b);
        assert!(
            score.similarity_percent >= 60.0,
            "minor body change should stay relatively similar, got {:.1}%",
            score.similarity_percent
        );
    }

    #[test]
    fn control_flow_change_detected() {
        let a = parse("fn f() { let x = 1; }", SupportedLanguage::Rust);
        let b = parse("fn f() { if true { let x = 1; } }", SupportedLanguage::Rust);
        let score = compute_similarity(&a, &b);
        assert!(score.control_flow_changed);
    }

    #[test]
    fn complexity_delta_positive_when_branch_added() {
        let a = parse("fn f() { let x = 1; }", SupportedLanguage::Rust);
        let b = parse("fn f() { if true { let x = 1; } }", SupportedLanguage::Rust);
        let score = compute_similarity(&a, &b);
        assert!(
            score.cyclomatic_delta > 0,
            "adding an if should increase complexity, delta = {}",
            score.cyclomatic_delta
        );
    }

    #[test]
    fn completely_different_code_low_similarity() {
        let a = parse("fn a() { let x = 1; }", SupportedLanguage::Rust);
        let b = parse(
            "struct S { field: String } impl S { fn method(&self) -> &str { &self.field } }",
            SupportedLanguage::Rust,
        );
        let score = compute_similarity(&a, &b);
        assert!(
            score.similarity_percent < 60.0,
            "completely different code should have low similarity, got {:.1}%",
            score.similarity_percent
        );
    }

    #[test]
    fn intensity_classification() {
        assert_eq!(classify_intensity(90.0), ChangeIntensity::Low);
        assert_eq!(classify_intensity(80.0), ChangeIntensity::Low);
        assert_eq!(classify_intensity(65.0), ChangeIntensity::Medium);
        assert_eq!(classify_intensity(50.0), ChangeIntensity::Medium);
        assert_eq!(classify_intensity(30.0), ChangeIntensity::High);
    }

    #[test]
    fn test_compute_positional_penalty_empty_ast() {
        let dummy = AstNode {
            id: 0,
            kind: "empty".to_string(),
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
            children: vec![],
            is_named: false,
        };
        assert_eq!(compute_positional_penalty(&dummy, &dummy), 0.0);
    }

    #[test]
    fn test_compute_positional_penalty_reversed_tokens() {
        let a = parse("fn test() { let a = 1; let b = 2; }", SupportedLanguage::Rust);
        let b = parse("fn test() { let b = 2; let a = 1; }", SupportedLanguage::Rust);
        let penalty = compute_positional_penalty(&a, &b);
        assert!(penalty > 0.0);
    }

    #[test]
    fn test_cyclomatic_complexity_for_loop() {
        let a = parse("fn f() { let x = 1; }", SupportedLanguage::Rust);
        let b = parse("fn f() { for i in 0..10 { println!(\"{}\", i); } }", SupportedLanguage::Rust);
        let score = compute_similarity(&a, &b);
        assert!(score.control_flow_changed);
        assert!(score.cyclomatic_delta > 0);
    }

    #[test]
    fn test_cyclomatic_complexity_while_loop() {
        let a = parse("fn f() { let x = 1; }", SupportedLanguage::Rust);
        let b = parse("fn f() { while true { break; } }", SupportedLanguage::Rust);
        let score = compute_similarity(&a, &b);
        assert!(score.control_flow_changed);
    }

    #[test]
    fn test_cyclomatic_complexity_match_arms() {
        let a = parse("fn f() { let x = 1; }", SupportedLanguage::Rust);
        let b = parse("fn f(x: i32) { match x { 1 => (), 2 => (), _ => () } }", SupportedLanguage::Rust);
        let score = compute_similarity(&a, &b);
        assert!(score.control_flow_changed);
        assert!(score.cyclomatic_delta >= 2);
    }

    #[test]
    fn test_change_intensity_display() {
        assert_eq!(ChangeIntensity::Low.to_string(), "low");
        assert_eq!(ChangeIntensity::Medium.to_string(), "medium");
        assert_eq!(ChangeIntensity::High.to_string(), "high");
    }

    #[test]
    fn test_count_nodes_nested() {
        let ast = parse("fn test() { let a = 1 + 2; }", SupportedLanguage::Rust);
        let count = count_nodes(&ast);
        assert!(count >= 5);
    }

    #[test]
    fn test_similarity_score_clamping() {
        let a = parse("fn a() {}", SupportedLanguage::Rust);
        let b = parse("fn b() {}", SupportedLanguage::Rust);
        let score = compute_similarity(&a, &b);
        assert!((0.0..=100.0).contains(&score.similarity_percent));
    }
}
