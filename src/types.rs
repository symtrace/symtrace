use serde::{Deserialize, Serialize};

// ── Supported Languages ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SupportedLanguage {
    Rust,
    JavaScript,
    TypeScript,
    Python,
    Java,
    C,
    Cpp,
    Go,
    Json,
    CSharp,
    Ruby,
    Php,
    Rust2024,
}

impl std::fmt::Display for SupportedLanguage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Rust => write!(f, "Rust"),
            Self::JavaScript => write!(f, "JavaScript"),
            Self::TypeScript => write!(f, "TypeScript"),
            Self::Python => write!(f, "Python"),
            Self::Java => write!(f, "Java"),
            Self::C => write!(f, "C"),
            Self::Cpp => write!(f, "C++"),
            Self::Go => write!(f, "Go"),
            Self::Json => write!(f, "JSON"),
            Self::CSharp => write!(f, "C#"),
            Self::Ruby => write!(f, "Ruby"),
            Self::Php => write!(f, "PHP"),
            Self::Rust2024 => write!(f, "Rust 2024"),
        }
    }
}

// ── File Change from Git ─────────────────────────────────────────────

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FileChange {
    pub path: String,
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub old_content: Option<String>,
    pub new_content: Option<String>,
    pub status: ChangeStatus,
    /// Git blob OID for the old version of the file (hex string)
    pub old_blob_hash: Option<String>,
    /// Git blob OID for the new version of the file (hex string)
    pub new_blob_hash: Option<String>,
}

impl FileChange {
    /// Return the active display path (prefers new_path, falls back to old_path or path)
    #[allow(dead_code)]
    pub fn display_path(&self) -> &str {
        self.new_path
            .as_deref()
            .or(self.old_path.as_deref())
            .unwrap_or(&self.path)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeStatus {
    Added,
    Deleted,
    Modified,
    Renamed,
}

// ── Internal AST Representation ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct AstNode {
    pub id: u64,
    pub kind: String,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_row: usize,
    pub start_col: usize,
    pub end_row: usize,
    pub end_col: usize,
    pub text: String,
    /// blake3(node_kind + ordered child structure_hashes) — pure tree shape
    pub structural_hash: [u8; 32],
    /// blake3(normalized_tokens) — leaf content with identifiers normalised
    pub content_hash: [u8; 32],
    /// blake3(parent_structure_hash + depth) — position in tree
    pub context_hash: [u8; 32],
    /// Legacy compatibility: structure with identifiers normalised for rename detection
    pub identity_hash: [u8; 32],
    pub children: Vec<AstNode>,
    pub is_named: bool,
}

// ── Operation Types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum OperationType {
    #[serde(rename = "MOVE")]
    Move,
    #[serde(rename = "RENAME")]
    Rename,
    #[serde(rename = "INSERT")]
    Insert,
    #[serde(rename = "DELETE")]
    Delete,
    #[serde(rename = "MODIFY")]
    Modify,
}

impl std::fmt::Display for OperationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Move => write!(f, "MOVE"),
            Self::Rename => write!(f, "RENAME"),
            Self::Insert => write!(f, "INSERT"),
            Self::Delete => write!(f, "DELETE"),
            Self::Modify => write!(f, "MODIFY"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EntityType {
    #[serde(rename = "function")]
    Function,
    #[serde(rename = "class")]
    Class,
    #[serde(rename = "variable")]
    Variable,
    #[serde(rename = "block")]
    Block,
    #[serde(rename = "other")]
    Other,
}

impl std::fmt::Display for EntityType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Function => write!(f, "function"),
            Self::Class => write!(f, "class"),
            Self::Variable => write!(f, "variable"),
            Self::Block => write!(f, "block"),
            Self::Other => write!(f, "other"),
        }
    }
}

// ── Operation Record ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct OperationRecord {
    #[serde(rename = "type")]
    pub op_type: OperationType,
    pub entity_type: EntityType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_location: Option<String>,
    pub details: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub similarity: Option<SimilarityScore>,
    #[serde(default = "default_true")]
    pub is_logic_op: bool,
}

#[inline]
fn default_true() -> bool {
    true
}

// ── Diff Output (JSON Schema) ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub file_path: String,
    pub operations: Vec<OperationRecord>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub refactor_patterns: Vec<RefactorPattern>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffSummary {
    pub total_files: usize,
    pub moves: usize,
    pub renames: usize,
    pub inserts: usize,
    pub deletes: usize,
    pub modifications: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceMetrics {
    pub total_files_processed: usize,
    pub total_nodes_compared: u64,
    pub parse_time_ms: f64,
    pub diff_time_ms: f64,
    pub total_time_ms: f64,
    /// Number of files parsed incrementally (tree reuse).
    #[serde(default)]
    pub incremental_parses: usize,
    /// Number of AST nodes whose hashes were reused from the old tree.
    #[serde(default)]
    pub nodes_reused: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DisplayGranularity {
    MicroCompact,
    Standard,
    FullStructural,
}

impl std::fmt::Display for DisplayGranularity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MicroCompact => write!(f, "micro_compact"),
            Self::Standard => write!(f, "standard"),
            Self::FullStructural => write!(f, "full_structural"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ImpactedCaller {
    pub caller_symbol: String,
    pub caller_file: String,
    pub call_site_line: usize,
    pub depth: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BlastRadiusReport {
    pub modified_symbol: String,
    pub file_path: String,
    pub total_impacted_callers: usize,
    pub impacted_callers: Vec<ImpactedCaller>,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ContractViolation {
    pub file_path: String,
    pub rule: String,
    pub message: String,
    pub line: usize,
    pub severity: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffOutput {
    pub repository: String,
    pub commit_a: String,
    pub commit_b: String,
    pub files: Vec<FileDiff>,
    pub summary: DiffSummary,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cross_file_tracking: Option<CrossFileTracking>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_classification: Option<CommitClassification>,
    pub performance: PerformanceMetrics,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub granularity: Option<DisplayGranularity>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blast_radius: Option<Vec<BlastRadiusReport>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contract_violations: Option<Vec<ContractViolation>>,
}

// ── Similarity Scoring ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeIntensity {
    Low,
    Medium,
    High,
}

impl std::fmt::Display for ChangeIntensity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Low => write!(f, "low"),
            Self::Medium => write!(f, "medium"),
            Self::High => write!(f, "high"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SimilarityScore {
    pub structure_similarity: f64,
    pub token_similarity: f64,
    pub node_count_delta: i64,
    pub cyclomatic_delta: i64,
    pub control_flow_changed: bool,
    pub similarity_percent: f64,
    pub change_intensity: ChangeIntensity,
}

// ── Refactor Pattern ─────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefactorKind {
    ExtractMethod,
    MoveMethod,
    RenameVariable,
}

impl std::fmt::Display for RefactorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExtractMethod => write!(f, "extract_method"),
            Self::MoveMethod => write!(f, "move_method"),
            Self::RenameVariable => write!(f, "rename_variable"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefactorPattern {
    pub kind: RefactorKind,
    pub description: String,
    pub involved_entities: Vec<String>,
    pub confidence: f64,
}

// ── Cross-File Symbol Tracking ────────────────────────────────────────

/// A unique identifier for a symbol in the global symbol table.
pub type SymbolId = u64;

/// An entry in the global symbol table, representing a named code entity
/// (function, class, method, variable) found in a specific file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolEntry {
    pub symbol_id: SymbolId,
    pub name: String,
    pub file_path: String,
    pub entity_type: EntityType,
    /// blake3 of signature (kind + name + child structure)
    pub signature_hash: [u8; 32],
    /// blake3 of the full subtree structure
    pub structure_hash: [u8; 32],
    /// Parent symbol id (0 if top-level)
    pub parent_symbol_id: SymbolId,
}

/// A cross-file matching event detected by comparing symbols across files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossFileEventKind {
    CrossFileMove,
    CrossFileRename,
    ApiSurfaceChange,
}

impl std::fmt::Display for CrossFileEventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CrossFileMove => write!(f, "cross_file_move"),
            Self::CrossFileRename => write!(f, "cross_file_rename"),
            Self::ApiSurfaceChange => write!(f, "api_surface_change"),
        }
    }
}

/// A detected cross-file symbol match / event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossFileMatch {
    pub event: CrossFileEventKind,
    pub old_symbol: String,
    pub old_file: String,
    pub new_symbol: String,
    pub new_file: String,
    pub similarity_score: f64,
    pub description: String,
}

/// The result of cross-file symbol tracking for the entire diff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossFileTracking {
    pub symbol_count: usize,
    pub cross_file_events: Vec<CrossFileMatch>,
}

// ── Commit Classification ────────────────────────────────────────────

/// The high-level classification of a commit's purpose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommitClass {
    Refactor,
    Feature,
    BugFix,
    Cleanup,
    FormattingOnly,
    Mixed,
}

impl std::fmt::Display for CommitClass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Refactor => write!(f, "refactor"),
            Self::Feature => write!(f, "feature"),
            Self::BugFix => write!(f, "bug_fix"),
            Self::Cleanup => write!(f, "cleanup"),
            Self::FormattingOnly => write!(f, "formatting_only"),
            Self::Mixed => write!(f, "mixed"),
        }
    }
}

/// The result of classifying a commit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitClassification {
    pub primary_class: CommitClass,
    pub confidence_score: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub intent_labels: Vec<String>,
}

// ── Parser Resource Limits ───────────────────────────────────────────

/// Configurable resource limits for the parser to prevent DoS from
/// pathological inputs. All limits are enforced during parsing and
/// exceeded limits cause the file to be skipped with a warning.
#[derive(Debug, Clone)]
pub struct ParserLimits {
    /// Maximum input file size in bytes (default: 5 MiB).
    pub max_file_size_bytes: usize,
    /// Maximum number of AST nodes before aborting (default: 200,000).
    pub max_ast_nodes: usize,
    /// Maximum recursion depth in the AST builder (default: 2,048).
    pub max_recursion_depth: usize,
    /// Optional parse timeout in milliseconds (default: 2,000; 0 = disabled).
    pub parse_timeout_ms: u64,
}

impl ParserLimits {
    pub fn compute_limits_hash(&self) -> u64 {
        let mut hasher = blake3::Hasher::new();
        hasher.update(&self.max_file_size_bytes.to_le_bytes());
        hasher.update(&self.max_ast_nodes.to_le_bytes());
        hasher.update(&self.max_recursion_depth.to_le_bytes());
        hasher.update(&self.parse_timeout_ms.to_le_bytes());
        let hash_bytes = hasher.finalize();
        u64::from_le_bytes(hash_bytes.as_bytes()[..8].try_into().unwrap())
    }
}

impl Default for ParserLimits {
    fn default() -> Self {
        Self {
            max_file_size_bytes: 5_242_880, // 5 MiB
            max_ast_nodes: 200_000,
            max_recursion_depth: 2_048,
            parse_timeout_ms: 2_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── SupportedLanguage Display ──────────────────────────────────────

    #[test]
    fn display_all_languages() {
        assert_eq!(SupportedLanguage::Rust.to_string(), "Rust");
        assert_eq!(SupportedLanguage::JavaScript.to_string(), "JavaScript");
        assert_eq!(SupportedLanguage::TypeScript.to_string(), "TypeScript");
        assert_eq!(SupportedLanguage::Python.to_string(), "Python");
        assert_eq!(SupportedLanguage::Java.to_string(), "Java");
    }

    // ── OperationType Display ──────────────────────────────────────────

    #[test]
    fn display_operation_types() {
        assert_eq!(OperationType::Move.to_string(), "MOVE");
        assert_eq!(OperationType::Rename.to_string(), "RENAME");
        assert_eq!(OperationType::Insert.to_string(), "INSERT");
        assert_eq!(OperationType::Delete.to_string(), "DELETE");
        assert_eq!(OperationType::Modify.to_string(), "MODIFY");
    }

    // ── EntityType Display ─────────────────────────────────────────────

    #[test]
    fn display_entity_types() {
        assert_eq!(EntityType::Function.to_string(), "function");
        assert_eq!(EntityType::Class.to_string(), "class");
        assert_eq!(EntityType::Variable.to_string(), "variable");
        assert_eq!(EntityType::Block.to_string(), "block");
        assert_eq!(EntityType::Other.to_string(), "other");
    }

    // ── SupportedLanguage PartialEq / Clone ───────────────────────────

    #[test]
    fn language_eq_and_clone() {
        let l = SupportedLanguage::Python;
        let cloned = l;
        assert_eq!(l, cloned);
        assert_ne!(l, SupportedLanguage::Rust);
    }

    // ── ChangeStatus ──────────────────────────────────────────────────

    #[test]
    fn change_status_variants() {
        let statuses = [
            ChangeStatus::Added,
            ChangeStatus::Deleted,
            ChangeStatus::Modified,
            ChangeStatus::Renamed,
        ];
        // Verify Debug impl doesn't panic
        for s in &statuses {
            let _ = format!("{s:?}");
        }
        assert_eq!(ChangeStatus::Added, ChangeStatus::Added);
        assert_ne!(ChangeStatus::Added, ChangeStatus::Deleted);
    }

    // ── OperationRecord JSON roundtrip ────────────────────────────────

    #[test]
    fn operation_record_json_roundtrip() {
        let record = OperationRecord {
            op_type: OperationType::Insert,
            entity_type: EntityType::Function,
            old_location: None,
            new_location: Some("L10".to_string()),
            details: "function_item 'foo' inserted".to_string(),
            similarity: None,
            is_logic_op: true,
        };
        let json = serde_json::to_string(&record).expect("serialize");
        let decoded: OperationRecord = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.op_type, OperationType::Insert);
        assert_eq!(decoded.entity_type, EntityType::Function);
        assert!(decoded.old_location.is_none());
        assert_eq!(decoded.new_location.as_deref(), Some("L10"));
        assert!(decoded.is_logic_op);
    }

    #[test]
    fn operation_record_old_location_omitted_when_none() {
        let record = OperationRecord {
            op_type: OperationType::Insert,
            entity_type: EntityType::Other,
            old_location: None,
            new_location: Some("L5".to_string()),
            details: "x".to_string(),
            similarity: None,
            is_logic_op: true,
        };
        let json = serde_json::to_string(&record).expect("serialize");
        assert!(
            !json.contains("\"old_location\""),
            "old_location should be skipped when None"
        );
    }

    #[test]
    fn operation_record_new_location_omitted_when_none() {
        let record = OperationRecord {
            op_type: OperationType::Delete,
            entity_type: EntityType::Other,
            old_location: Some("L3".to_string()),
            new_location: None,
            details: "y".to_string(),
            similarity: None,
            is_logic_op: true,
        };
        let json = serde_json::to_string(&record).expect("serialize");
        assert!(
            !json.contains("\"new_location\""),
            "new_location should be skipped when None"
        );
    }

    // ── OperationType serde rename ────────────────────────────────────

    #[test]
    fn operation_type_serialises_to_uppercase() {
        let pairs = [
            (OperationType::Move, "\"MOVE\""),
            (OperationType::Rename, "\"RENAME\""),
            (OperationType::Insert, "\"INSERT\""),
            (OperationType::Delete, "\"DELETE\""),
            (OperationType::Modify, "\"MODIFY\""),
        ];
        for (op, expected) in &pairs {
            let s = serde_json::to_string(op).unwrap();
            assert_eq!(&s, expected, "OperationType::{op:?} serialised wrong");
        }
    }

    // ── EntityType serde rename ───────────────────────────────────────

    #[test]
    fn entity_type_serialises_to_lowercase() {
        let pairs = [
            (EntityType::Function, "\"function\""),
            (EntityType::Class, "\"class\""),
            (EntityType::Variable, "\"variable\""),
            (EntityType::Block, "\"block\""),
            (EntityType::Other, "\"other\""),
        ];
        for (e, expected) in &pairs {
            let s = serde_json::to_string(e).unwrap();
            assert_eq!(&s, expected, "EntityType::{e:?} serialised wrong");
        }
    }

    // ── DiffSummary ───────────────────────────────────────────────────

    #[test]
    fn diff_summary_json_roundtrip() {
        let summary = DiffSummary {
            total_files: 3,
            moves: 1,
            renames: 2,
            inserts: 5,
            deletes: 4,
            modifications: 7,
        };
        let json = serde_json::to_string(&summary).unwrap();
        let decoded: DiffSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.total_files, 3);
        assert_eq!(decoded.moves, 1);
        assert_eq!(decoded.renames, 2);
        assert_eq!(decoded.inserts, 5);
        assert_eq!(decoded.deletes, 4);
        assert_eq!(decoded.modifications, 7);
    }

    // ── FileChange ────────────────────────────────────────────────────

    #[test]
    fn file_change_clone() {
        let fc = FileChange {
            path: "src/lib.rs".to_string(),
            old_path: Some("src/lib.rs".to_string()),
            new_path: Some("src/lib.rs".to_string()),
            old_content: Some("fn a() {}".to_string()),
            new_content: Some("fn b() {}".to_string()),
            status: ChangeStatus::Modified,
            old_blob_hash: Some("abc123".to_string()),
            new_blob_hash: Some("def456".to_string()),
        };
        let cloned = fc.clone();
        assert_eq!(cloned.path, "src/lib.rs");
        assert_eq!(cloned.status, ChangeStatus::Modified);
    }

    #[test]
    fn test_parser_limits_default_and_hash() {
        let limits = ParserLimits::default();
        assert_eq!(limits.max_file_size_bytes, 5_242_880);
        let h1 = limits.compute_limits_hash();
        let h2 = limits.compute_limits_hash();
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_commit_class_display_all_variants() {
        assert_eq!(CommitClass::Refactor.to_string(), "refactor");
        assert_eq!(CommitClass::Feature.to_string(), "feature");
        assert_eq!(CommitClass::BugFix.to_string(), "bug_fix");
        assert_eq!(CommitClass::Cleanup.to_string(), "cleanup");
        assert_eq!(CommitClass::FormattingOnly.to_string(), "formatting_only");
        assert_eq!(CommitClass::Mixed.to_string(), "mixed");
    }

    #[test]
    fn test_display_granularity_display_all() {
        assert_eq!(DisplayGranularity::MicroCompact.to_string(), "micro_compact");
        assert_eq!(DisplayGranularity::Standard.to_string(), "standard");
        assert_eq!(DisplayGranularity::FullStructural.to_string(), "full_structural");
    }

    #[test]
    fn test_blast_radius_report_serialization() {
        let report = BlastRadiusReport {
            modified_symbol: "login".to_string(),
            file_path: "src/auth.rs".to_string(),
            total_impacted_callers: 2,
            severity: "HIGH".to_string(),
            impacted_callers: vec![ImpactedCaller {
                caller_symbol: "handler".to_string(),
                caller_file: "src/api.rs".to_string(),
                call_site_line: 42,
                depth: 1,
            }],
        };
        let json = serde_json::to_string(&report).unwrap();
        let de: BlastRadiusReport = serde_json::from_str(&json).unwrap();
        assert_eq!(de.modified_symbol, "login");
        assert_eq!(de.total_impacted_callers, 2);
    }

    #[test]
    fn test_contract_violation_serialization() {
        let cv = ContractViolation {
            file_path: "src/ptr.c".to_string(),
            rule: "REMOVED_NULL_CHECK".to_string(),
            message: "Null check removed".to_string(),
            line: 10,
            severity: "CRITICAL".to_string(),
        };
        let json = serde_json::to_string(&cv).unwrap();
        let de: ContractViolation = serde_json::from_str(&json).unwrap();
        assert_eq!(de.rule, "REMOVED_NULL_CHECK");
    }

    #[test]
    fn test_change_status_debug() {
        assert_eq!(format!("{:?}", ChangeStatus::Added), "Added");
        assert_eq!(format!("{:?}", ChangeStatus::Deleted), "Deleted");
        assert_eq!(format!("{:?}", ChangeStatus::Modified), "Modified");
        assert_eq!(format!("{:?}", ChangeStatus::Renamed), "Renamed");
    }

    #[test]
    fn test_file_diff_empty() {
        let fd = FileDiff {
            file_path: "src/empty.rs".to_string(),
            operations: vec![],
            refactor_patterns: vec![],
        };
        assert!(fd.operations.is_empty());
        assert!(fd.refactor_patterns.is_empty());
    }

    #[test]
    fn test_supported_language_display_variants() {
        assert_eq!(SupportedLanguage::Rust.to_string(), "Rust");
        assert_eq!(SupportedLanguage::Python.to_string(), "Python");
        assert_eq!(SupportedLanguage::TypeScript.to_string(), "TypeScript");
        assert_eq!(SupportedLanguage::C.to_string(), "C");
        assert_eq!(SupportedLanguage::Cpp.to_string(), "C++");
    }
}
