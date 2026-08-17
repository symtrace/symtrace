//! Commit Classification
//!
//! Analyses all file diffs, operations, and refactor patterns to classify
//! a commit into one of six categories:
//!
//! | Class            | Key Signals                                                    |
//! |------------------|----------------------------------------------------------------|
//! | `refactor`       | rename_count > insert_count, avg similarity > 0.8, low Δ CC   |
//! | `feature`        | high insert count, new public symbols detected                 |
//! | `bug_fix`        | modifications dominate, small scope, control-flow changes      |
//! | `cleanup`        | logic_only produces no changes, formatting changes detected    |
//! | `formatting_only`| zero semantic operations                                       |
//! | `mixed`          | no single class dominates                                      |
//!
//! Output: `primary_class` plus a `confidence_score` ∈ [0, 1].

use crate::types::{CommitClass, CommitClassification, DiffSummary, FileDiff, OperationType};

// ── Thresholds ───────────────────────────────────────────────────────

/// Average similarity above this means the changes are structure-preserving.
const HIGH_SIMILARITY_THRESHOLD: f64 = 0.80;
/// When insert ratio exceeds this, it's feature-like.
const INSERT_RATIO_THRESHOLD: f64 = 0.50;
/// Maximum absolute cyclomatic delta to be considered "small".
const COMPLEXITY_DELTA_SMALL: i64 = 2;
/// Minimum ratio of modifications for bug_fix heuristic.
const BUG_FIX_MODIFY_RATIO: f64 = 0.60;

// ── Public API ───────────────────────────────────────────────────────

/// Classify a commit based on its file diffs and summary.
///
/// `logic_only_ops_empty`: whether running in logic-only mode would produce
/// zero operations (indicating only comments/formatting changed).
pub fn classify_commit(
    file_diffs: &[FileDiff],
    summary: &DiffSummary,
    logic_only_no_changes: bool,
) -> CommitClassification {
    // ── Gather metrics ───────────────────────────────────────────────
    let total_ops =
        summary.moves + summary.renames + summary.inserts + summary.deletes + summary.modifications;

    // If there are zero ops at all, it's formatting_only
    if total_ops == 0 {
        return CommitClassification {
            primary_class: CommitClass::FormattingOnly,
            confidence_score: 1.0,
            intent_labels: vec![],
        };
    }

    let rename_count = summary.renames;
    let insert_count = summary.inserts;
    let delete_count = summary.deletes;
    let modify_count = summary.modifications;
    let move_count = summary.moves;

    // Compute average similarity across all operations that have one
    let (avg_similarity, _sim_count) = compute_avg_similarity(file_diffs);

    // Compute average cyclomatic delta
    let avg_complexity_delta = compute_avg_complexity_delta(file_diffs);

    // Count refactor patterns
    let refactor_pattern_count = count_refactor_patterns(file_diffs);

    // Check for new public symbols (INSERT of function/class entities)
    let new_public_symbols = count_new_public_symbols(file_diffs);

    // Check if control flow changed in any modification
    let control_flow_changes = count_control_flow_changes(file_diffs);

    // ── Score each class ─────────────────────────────────────────────
    let mut scores: Vec<(CommitClass, f64)> = Vec::new();

    // ── Cleanup / formatting_only ────────────────────────────────────
    if logic_only_no_changes {
        // All changes are comments/whitespace only
        let confidence = if total_ops == 0 { 1.0 } else { 0.85 };
        return CommitClassification {
            primary_class: CommitClass::Cleanup,
            confidence_score: confidence,
            intent_labels: vec![],
        };
    }

    // ── Refactor score ───────────────────────────────────────────────
    {
        let mut score = 0.0;

        // rename_count > insert_count
        if rename_count > insert_count {
            score += 0.30;
        }
        // High average similarity
        if avg_similarity > HIGH_SIMILARITY_THRESHOLD {
            score += 0.25;
        }
        // Small complexity delta
        if avg_complexity_delta.abs() <= COMPLEXITY_DELTA_SMALL as f64 {
            score += 0.15;
        }
        // Moves present (indicates restructuring)
        if move_count > 0 {
            score += 0.15;
        }
        // Refactor patterns detected
        if refactor_pattern_count > 0 {
            score += 0.15;
        }

        scores.push((CommitClass::Refactor, score));
    }

    // ── Feature score ────────────────────────────────────────────────
    {
        let mut score = 0.0;
        let insert_ratio = if total_ops > 0 {
            insert_count as f64 / total_ops as f64
        } else {
            0.0
        };

        // High insert count / ratio
        if insert_ratio > INSERT_RATIO_THRESHOLD {
            score += 0.40;
        } else if insert_count > 0 {
            score += 0.15;
        }

        // New public symbols detected
        if new_public_symbols > 0 {
            score += 0.35;
        }

        // Low delete count relative to inserts
        if insert_count > delete_count * 2 {
            score += 0.15;
        }

        // Some modifications (not pure inserts)
        if modify_count > 0 && insert_count > modify_count {
            score += 0.10;
        }

        scores.push((CommitClass::Feature, score));
    }

    // ── Bug fix score ────────────────────────────────────────────────
    {
        let mut score = 0.0;
        let modify_ratio = if total_ops > 0 {
            modify_count as f64 / total_ops as f64
        } else {
            0.0
        };

        // Modifications dominate
        if modify_ratio >= BUG_FIX_MODIFY_RATIO {
            score += 0.30;
        }

        // Small scope (few files, few total ops)
        if summary.total_files <= 3 && total_ops <= 10 {
            score += 0.20;
        }

        // Control flow changed (typical in bug fixes)
        if control_flow_changes > 0 {
            score += 0.25;
        }

        // Low insert count
        if insert_count == 0 {
            score += 0.15;
        }

        // Moderate similarity (not a full rewrite)
        if avg_similarity > 0.5 && avg_similarity < HIGH_SIMILARITY_THRESHOLD {
            score += 0.10;
        }

        scores.push((CommitClass::BugFix, score));
    }

    // ── Cleanup score ────────────────────────────────────────────────
    {
        let mut score = 0.0;
        let delete_ratio = if total_ops > 0 {
            delete_count as f64 / total_ops as f64
        } else {
            0.0
        };

        // Mostly deletes
        if delete_ratio > 0.5 {
            score += 0.30;
        }

        // Very high similarity (minor tweaks)
        if avg_similarity > 0.90 {
            score += 0.25;
        }

        // No new public symbols
        if new_public_symbols == 0 {
            score += 0.15;
        }

        // No control flow changes
        if control_flow_changes == 0 {
            score += 0.15;
        }

        // Small complexity delta
        if avg_complexity_delta.abs() <= COMPLEXITY_DELTA_SMALL as f64 {
            score += 0.15;
        }

        scores.push((CommitClass::Cleanup, score));
    }

    // ── Select winner ────────────────────────────────────────────────
    scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let (best_class, best_score) = scores[0].clone();
    let second_score = if scores.len() > 1 { scores[1].1 } else { 0.0 };

    let intent_labels = extract_intent_labels(file_diffs);

    // If top two scores are very close, classify as Mixed
    let margin = best_score - second_score;
    if margin < 0.10 && best_score > 0.0 {
        CommitClassification {
            primary_class: CommitClass::Mixed,
            confidence_score: (best_score * 0.8).clamp(0.0, 1.0),
            intent_labels,
        }
    } else {
        CommitClassification {
            primary_class: best_class,
            confidence_score: best_score.clamp(0.0, 1.0),
            intent_labels,
        }
    }
}

fn extract_intent_labels(file_diffs: &[FileDiff]) -> Vec<String> {
    let mut labels = Vec::new();

    for fd in file_diffs {
        for op in &fd.operations {
            let details_lower = op.details.to_lowercase();
            if details_lower.contains("guard") || details_lower.contains("null check") || details_lower.contains("bounds") {
                if !labels.contains(&"GUARD_CLAUSE_ADDED".to_string()) {
                    labels.push("GUARD_CLAUSE_ADDED".to_string());
                }
            }
            if details_lower.contains("signature") || details_lower.contains("param") || details_lower.contains("return type") {
                if !labels.contains(&"TYPE_SIGNATURE_CHANGED".to_string()) {
                    labels.push("TYPE_SIGNATURE_CHANGED".to_string());
                }
            }
            if let Some(ref sim) = op.similarity {
                if sim.control_flow_changed && !labels.contains(&"CONTROL_FLOW_INVERTED".to_string()) {
                    labels.push("CONTROL_FLOW_INVERTED".to_string());
                }
            }
            if details_lower.contains("close") || details_lower.contains("drop") || details_lower.contains("cleanup") || details_lower.contains("free") {
                if !labels.contains(&"RESOURCE_CLEANUP_ADDED".to_string()) {
                    labels.push("RESOURCE_CLEANUP_ADDED".to_string());
                }
            }
        }
    }

    labels
}

// ── Metric computation helpers ───────────────────────────────────────

/// Compute the average similarity percentage across all operations that
/// contain a similarity score.
fn compute_avg_similarity(file_diffs: &[FileDiff]) -> (f64, usize) {
    let mut total = 0.0;
    let mut count = 0usize;
    for fd in file_diffs {
        for op in &fd.operations {
            if let Some(ref sim) = op.similarity {
                total += sim.similarity_percent / 100.0;
                count += 1;
            }
        }
    }
    if count == 0 {
        (0.5, 0) // neutral default
    } else {
        (total / count as f64, count)
    }
}

/// Compute the average absolute cyclomatic complexity delta.
fn compute_avg_complexity_delta(file_diffs: &[FileDiff]) -> f64 {
    let mut total: f64 = 0.0;
    let mut count = 0usize;
    for fd in file_diffs {
        for op in &fd.operations {
            if let Some(ref sim) = op.similarity {
                total += sim.cyclomatic_delta as f64;
                count += 1;
            }
        }
    }
    if count == 0 {
        0.0
    } else {
        total / count as f64
    }
}

/// Count the total number of refactor patterns detected.
fn count_refactor_patterns(file_diffs: &[FileDiff]) -> usize {
    file_diffs.iter().map(|fd| fd.refactor_patterns.len()).sum()
}

/// Count newly inserted public symbols (functions & classes).
fn count_new_public_symbols(file_diffs: &[FileDiff]) -> usize {
    file_diffs
        .iter()
        .flat_map(|fd| fd.operations.iter())
        .filter(|op| {
            op.op_type == OperationType::Insert
                && matches!(
                    op.entity_type,
                    crate::types::EntityType::Function | crate::types::EntityType::Class
                )
        })
        .count()
}

/// Count operations where control flow changed.
fn count_control_flow_changes(file_diffs: &[FileDiff]) -> usize {
    file_diffs
        .iter()
        .flat_map(|fd| fd.operations.iter())
        .filter(|op| {
            op.similarity
                .as_ref()
                .is_some_and(|s| s.control_flow_changed)
        })
        .count()
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        ChangeIntensity, DiffSummary, EntityType, FileDiff, OperationRecord, OperationType,
        RefactorKind, RefactorPattern, SimilarityScore,
    };

    fn make_summary(
        files: usize,
        moves: usize,
        renames: usize,
        inserts: usize,
        deletes: usize,
        mods: usize,
    ) -> DiffSummary {
        DiffSummary {
            total_files: files,
            moves,
            renames,
            inserts,
            deletes,
            modifications: mods,
        }
    }

    fn make_op(
        op_type: OperationType,
        entity: EntityType,
        details: &str,
        similarity: Option<SimilarityScore>,
    ) -> OperationRecord {
        OperationRecord {
            op_type,
            entity_type: entity,
            old_location: Some("L1".to_string()),
            new_location: Some("L5".to_string()),
            details: details.to_string(),
            similarity,
            is_logic_op: true,
        }
    }

    fn high_similarity() -> SimilarityScore {
        SimilarityScore {
            structure_similarity: 0.95,
            token_similarity: 0.90,
            node_count_delta: 0,
            cyclomatic_delta: 0,
            control_flow_changed: false,
            similarity_percent: 92.0,
            change_intensity: ChangeIntensity::Low,
        }
    }

    fn medium_similarity() -> SimilarityScore {
        SimilarityScore {
            structure_similarity: 0.70,
            token_similarity: 0.65,
            node_count_delta: 3,
            cyclomatic_delta: 1,
            control_flow_changed: true,
            similarity_percent: 68.0,
            change_intensity: ChangeIntensity::Medium,
        }
    }

    // ── Formatting only ──────────────────────────────────────────────

    #[test]
    fn no_ops_is_formatting_only() {
        let diffs = vec![];
        let summary = make_summary(1, 0, 0, 0, 0, 0);
        let result = classify_commit(&diffs, &summary, false);
        assert_eq!(result.primary_class, CommitClass::FormattingOnly);
        assert!((result.confidence_score - 1.0).abs() < f64::EPSILON);
    }

    // ── Cleanup (logic_only no changes) ──────────────────────────────

    #[test]
    fn logic_only_no_changes_is_cleanup() {
        let diffs = vec![FileDiff {
            file_path: "test.rs".to_string(),
            operations: vec![make_op(
                OperationType::Modify,
                EntityType::Function,
                "fn foo modified",
                Some(high_similarity()),
            )],
            refactor_patterns: vec![],
        }];
        let summary = make_summary(1, 0, 0, 0, 0, 1);
        let result = classify_commit(&diffs, &summary, true);
        assert_eq!(result.primary_class, CommitClass::Cleanup);
    }

    // ── Feature ──────────────────────────────────────────────────────

    #[test]
    fn many_inserts_classified_as_feature() {
        let diffs = vec![FileDiff {
            file_path: "new.rs".to_string(),
            operations: vec![
                make_op(
                    OperationType::Insert,
                    EntityType::Function,
                    "fn alpha inserted",
                    None,
                ),
                make_op(
                    OperationType::Insert,
                    EntityType::Function,
                    "fn beta inserted",
                    None,
                ),
                make_op(
                    OperationType::Insert,
                    EntityType::Class,
                    "struct Gamma inserted",
                    None,
                ),
            ],
            refactor_patterns: vec![],
        }];
        let summary = make_summary(1, 0, 0, 3, 0, 0);
        let result = classify_commit(&diffs, &summary, false);
        assert_eq!(
            result.primary_class,
            CommitClass::Feature,
            "many inserts should be classified as Feature, got: {:?} ({:.2})",
            result.primary_class,
            result.confidence_score
        );
    }

    // ── Refactor ─────────────────────────────────────────────────────

    #[test]
    fn renames_with_high_similarity_classified_as_refactor() {
        let diffs = vec![FileDiff {
            file_path: "lib.rs".to_string(),
            operations: vec![
                make_op(
                    OperationType::Rename,
                    EntityType::Function,
                    "fn renamed",
                    Some(high_similarity()),
                ),
                make_op(
                    OperationType::Rename,
                    EntityType::Variable,
                    "var renamed",
                    Some(high_similarity()),
                ),
                make_op(
                    OperationType::Move,
                    EntityType::Function,
                    "fn moved",
                    Some(high_similarity()),
                ),
            ],
            refactor_patterns: vec![RefactorPattern {
                kind: RefactorKind::RenameVariable,
                description: "x renamed to y".to_string(),
                involved_entities: vec!["x".to_string(), "y".to_string()],
                confidence: 1.0,
            }],
        }];
        let summary = make_summary(1, 1, 2, 0, 0, 0);
        let result = classify_commit(&diffs, &summary, false);
        assert_eq!(
            result.primary_class,
            CommitClass::Refactor,
            "renames + high similarity should be Refactor, got: {:?} ({:.2})",
            result.primary_class,
            result.confidence_score
        );
    }

    // ── Bug fix ──────────────────────────────────────────────────────

    #[test]
    fn modifications_with_control_flow_change_classified_as_bug_fix() {
        let diffs = vec![FileDiff {
            file_path: "handler.rs".to_string(),
            operations: vec![
                make_op(
                    OperationType::Modify,
                    EntityType::Function,
                    "fn process modified",
                    Some(medium_similarity()),
                ),
                make_op(
                    OperationType::Modify,
                    EntityType::Function,
                    "fn validate modified",
                    Some(medium_similarity()),
                ),
            ],
            refactor_patterns: vec![],
        }];
        let summary = make_summary(1, 0, 0, 0, 0, 2);
        let result = classify_commit(&diffs, &summary, false);
        assert_eq!(
            result.primary_class,
            CommitClass::BugFix,
            "modifications with control flow changes should be BugFix, got: {:?} ({:.2})",
            result.primary_class,
            result.confidence_score
        );
    }

    // ── Commit class display ─────────────────────────────────────────

    #[test]
    fn commit_class_display() {
        assert_eq!(CommitClass::Refactor.to_string(), "refactor");
        assert_eq!(CommitClass::Feature.to_string(), "feature");
        assert_eq!(CommitClass::BugFix.to_string(), "bug_fix");
        assert_eq!(CommitClass::Cleanup.to_string(), "cleanup");
        assert_eq!(CommitClass::FormattingOnly.to_string(), "formatting_only");
        assert_eq!(CommitClass::Mixed.to_string(), "mixed");
    }

    // ── Confidence score is bounded ──────────────────────────────────

    #[test]
    fn confidence_score_is_bounded() {
        let diffs = vec![FileDiff {
            file_path: "x.rs".to_string(),
            operations: vec![make_op(
                OperationType::Insert,
                EntityType::Function,
                "fn a inserted",
                None,
            )],
            refactor_patterns: vec![],
        }];
        let summary = make_summary(1, 0, 0, 1, 0, 0);
        let result = classify_commit(&diffs, &summary, false);
        assert!(
            result.confidence_score >= 0.0 && result.confidence_score <= 1.0,
            "confidence should be in [0,1], got: {}",
            result.confidence_score
        );
    }

    // ── JSON roundtrip ───────────────────────────────────────────────

    #[test]
    fn classification_json_roundtrip() {
        let c = CommitClassification {
            primary_class: CommitClass::Feature,
            confidence_score: 0.85,
            intent_labels: vec!["TYPE_SIGNATURE_CHANGED".to_string()],
        };
        let json = serde_json::to_string(&c).unwrap();
        let decoded: CommitClassification = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.primary_class, CommitClass::Feature);
        assert!((decoded.confidence_score - 0.85).abs() < f64::EPSILON);
        assert_eq!(decoded.intent_labels, vec!["TYPE_SIGNATURE_CHANGED"]);
    }
}
