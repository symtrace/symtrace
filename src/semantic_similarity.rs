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

    let composite = structure_sim * STRUCTURE_WEIGHT
        + token_sim * TOKEN_WEIGHT
        + complexity_factor * COMPLEXITY_WEIGHT;

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
        assert_eq!(classify_intensity(65.0), ChangeIntensity::Medium);
        assert_eq!(classify_intensity(30.0), ChangeIntensity::High);
    }
}
