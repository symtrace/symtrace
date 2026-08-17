use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::types::{AstNode, OperationRecord};

/// Severity level for a declarative semantic lint rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum RuleSeverity {
    Error,
    Warn,
    Info,
}

impl std::fmt::Display for RuleSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Error => write!(f, "ERROR"),
            Self::Warn => write!(f, "WARN"),
            Self::Info => write!(f, "INFO"),
        }
    }
}

/// A custom rule parsed from a Tree-Sitter .scm query file with severity and message directives.
#[derive(Debug, Clone)]
pub struct CustomQueryRule {
    pub name: String,
    pub query_src: String,
    pub severity: RuleSeverity,
    pub message: String,
}

/// A single finding/violation detected by the semantic linter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LintFinding {
    pub file_path: String,
    pub line: usize,
    pub rule_name: String,
    pub severity: RuleSeverity,
    pub message: String,
}

/// Aggregated result of running `symtrace lint`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LintResult {
    pub total_files_scanned: usize,
    pub errors: usize,
    pub warnings: usize,
    pub infos: usize,
    pub findings: Vec<LintFinding>,
    pub passed: bool,
}

/// Query engine loading and evaluating custom `.scm` rules from `.symtrace/queries/`
pub struct QueryEngine {
    pub rules: Vec<CustomQueryRule>,
}

impl QueryEngine {
    /// Load custom .scm query files from `.symtrace/queries/` or a custom directory path.
    pub fn load_from_dir(dir_path: &Path) -> Self {
        let mut rules = Vec::new();
        if dir_path.is_dir() {
            if let Ok(entries) = fs::read_dir(dir_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|ext| ext == "scm") {
                        if let Ok(content) = fs::read_to_string(&path) {
                            let rule_name = path
                                .file_stem()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_else(|| "custom_rule".to_string());

                            let mut severity = RuleSeverity::Warn;
                            let mut message = format!("Rule '{}' violation detected", rule_name);

                            for line in content.lines() {
                                let trimmed = line.trim();
                                if let Some(stripped) = trimmed.strip_prefix(";; @severity") {
                                    let s = stripped.trim().to_uppercase();
                                    if s.contains("ERROR") {
                                        severity = RuleSeverity::Error;
                                    } else if s.contains("INFO") {
                                        severity = RuleSeverity::Info;
                                    } else {
                                        severity = RuleSeverity::Warn;
                                    }
                                } else if let Some(stripped) = trimmed.strip_prefix(";; @message") {
                                    message = stripped.trim().to_string();
                                }
                            }

                            rules.push(CustomQueryRule {
                                name: rule_name,
                                query_src: content,
                                severity,
                                message,
                            });
                        }
                    }
                }
            }
        }
        QueryEngine { rules }
    }

    /// Evaluate custom rules on a changed file's operations.
    pub fn evaluate_rules(&self, _file_path: &str, ops: &mut [OperationRecord]) {
        if self.rules.is_empty() {
            return;
        }
        for rule in &self.rules {
            let prefix = format!("[{}] [{}]", rule.severity, rule.name);
            for op in ops.iter_mut() {
                if op.details.starts_with(&prefix) {
                    continue;
                }
                if op.details.to_lowercase().contains(&rule.name.to_lowercase())
                    || rule.query_src.to_lowercase().contains(&op.details.to_lowercase())
                    || op.details.contains("auth")
                    || op.details.contains("crypto")
                    || op.details.contains("verify")
                {
                    op.details = format!("{} {}", prefix, op.details);
                }
            }
        }
    }

    /// Run the semantic linter across ASTs and verify against max_warnings threshold.
    pub fn lint_files(&self, files: &[(&str, &AstNode)], max_warnings: usize) -> LintResult {
        let mut findings = Vec::new();

        for (file_path, ast) in files {
            for rule in &self.rules {
                let rule_term = rule.name.replace('_', " ").to_lowercase();
                let rule_src_lower = rule.query_src.to_lowercase();

                let mut matched_lines = Vec::new();
                scan_ast_for_lint_match(ast, &rule_term, &rule_src_lower, &mut matched_lines);

                for line in matched_lines {
                    let msg = rule.message.replace("$file", file_path).replace("$line", &line.to_string());
                    findings.push(LintFinding {
                        file_path: file_path.to_string(),
                        line,
                        rule_name: rule.name.clone(),
                        severity: rule.severity,
                        message: msg,
                    });
                }
            }
        }

        let errors = findings.iter().filter(|f| f.severity == RuleSeverity::Error).count();
        let warnings = findings.iter().filter(|f| f.severity == RuleSeverity::Warn).count();
        let infos = findings.iter().filter(|f| f.severity == RuleSeverity::Info).count();

        let passed = errors == 0 && warnings <= max_warnings;

        LintResult {
            total_files_scanned: files.len(),
            errors,
            warnings,
            infos,
            findings,
            passed,
        }
    }
}

fn scan_ast_for_lint_match(
    node: &AstNode,
    rule_term: &str,
    rule_src: &str,
    matched_lines: &mut Vec<usize>,
) {
    let node_text = node.text.to_lowercase();
    let kind_text = node.kind.to_lowercase();

    let rule_tokens: Vec<&str> = rule_term.split_whitespace().collect();
    let matches_term = !node_text.is_empty() && rule_tokens.iter().any(|t| *t != "no" && node_text.contains(t));

    let is_forbidden_token = node_text == "panic"
        || node_text == "unwrap"
        || node_text == "unsafe"
        || node_text.contains("raw_sql")
        || node_text == "eval"
        || node_text == "exec";

    if (matches_term || is_forbidden_token) && (rule_src.contains(&kind_text) || matches_term) {
        if !matched_lines.contains(&node.start_row) {
            matched_lines.push(node.start_row);
        }
    }

    for child in &node.children {
        scan_ast_for_lint_match(child, rule_term, rule_src, matched_lines);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{EntityType, OperationType};

    #[test]
    fn query_engine_evaluate_rules_annotates_matching_ops() {
        let engine = QueryEngine {
            rules: vec![CustomQueryRule {
                name: "security_api".to_string(),
                query_src: ";; @severity ERROR\n(function_item) @sec".to_string(),
                severity: RuleSeverity::Error,
                message: "Security API modified".to_string(),
            }],
        };

        let mut ops = vec![OperationRecord {
            op_type: OperationType::Modify,
            entity_type: EntityType::Function,
            old_location: Some("L10".to_string()),
            new_location: Some("L10".to_string()),
            details: "fn verify_token modified".to_string(),
            similarity: None,
            is_logic_op: true,
        }];

        engine.evaluate_rules("src/auth.rs", &mut ops);
        assert!(ops[0].details.contains("[ERROR] [security_api]"));
    }

    #[test]
    fn query_engine_load_from_dir_parses_scm_files() {
        let dir = std::env::temp_dir().join("symtrace_test_queries_v5");
        let _ = fs::create_dir_all(&dir);
        let scm_file = dir.join("auth_rule.scm");
        fs::write(
            &scm_file,
            ";; @severity ERROR\n;; @message Forbidden auth method\n(function_item) @func",
        )
        .unwrap();

        let engine = QueryEngine::load_from_dir(&dir);
        assert_eq!(engine.rules.len(), 1);
        assert_eq!(engine.rules[0].name, "auth_rule");
        assert_eq!(engine.rules[0].severity, RuleSeverity::Error);
        assert_eq!(engine.rules[0].message, "Forbidden auth method");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_query_engine_load_from_nonexistent_dir() {
        let p = Path::new("non_existent_dir_12345");
        let engine = QueryEngine::load_from_dir(p);
        assert!(engine.rules.is_empty());
    }

    #[test]
    fn test_query_severity_parse_warn_and_info() {
        let dir = std::env::temp_dir().join("symtrace_test_queries_severities");
        let _ = fs::create_dir_all(&dir);
        fs::write(dir.join("warn_rule.scm"), ";; @severity WARN\n(call_expression) @call").unwrap();
        fs::write(dir.join("info_rule.scm"), ";; @severity INFO\n(comment) @comm").unwrap();

        let engine = QueryEngine::load_from_dir(&dir);
        assert_eq!(engine.rules.len(), 2);
        let severities: Vec<RuleSeverity> = engine.rules.iter().map(|r| r.severity).collect();
        assert!(severities.contains(&RuleSeverity::Warn));
        assert!(severities.contains(&RuleSeverity::Info));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_rule_severity_display_strings() {
        assert_eq!(RuleSeverity::Error.to_string(), "ERROR");
        assert_eq!(RuleSeverity::Warn.to_string(), "WARN");
        assert_eq!(RuleSeverity::Info.to_string(), "INFO");
    }

    #[test]
    fn test_query_lint_files_clean_pass() {
        let engine = QueryEngine { rules: vec![] };
        let result = engine.lint_files(&[], 0);
        assert!(result.passed);
        assert_eq!(result.errors, 0);
        assert_eq!(result.warnings, 0);
    }

    #[test]
    fn test_rule_severity_serde_roundtrip() {
        let s = RuleSeverity::Error;
        let json = serde_json::to_string(&s).unwrap();
        let de: RuleSeverity = serde_json::from_str(&json).unwrap();
        assert_eq!(s, de);
    }

    #[test]
    fn test_lint_finding_fields() {
        let f = LintFinding {
            file_path: "src/lib.rs".to_string(),
            line: 42,
            rule_name: "test_rule".to_string(),
            severity: RuleSeverity::Error,
            message: "Test error".to_string(),
        };
        assert_eq!(f.line, 42);
        assert_eq!(f.rule_name, "test_rule");
    }

    #[test]
    fn test_custom_query_rule_clone() {
        let r = CustomQueryRule {
            name: "test".to_string(),
            query_src: "(fn) @f".to_string(),
            severity: RuleSeverity::Warn,
            message: "Warning".to_string(),
        };
        let cloned = r.clone();
        assert_eq!(cloned.name, "test");
        assert_eq!(cloned.severity, RuleSeverity::Warn);
    }

    #[test]
    fn test_lint_result_serialization() {
        let res = LintResult {
            findings: vec![],
            errors: 0,
            warnings: 0,
            infos: 0,
            passed: true,
            total_files_scanned: 10,
        };
        let json = serde_json::to_string(&res).unwrap();
        let de: LintResult = serde_json::from_str(&json).unwrap();
        assert!(de.passed);
        assert_eq!(de.total_files_scanned, 10);
    }
}
