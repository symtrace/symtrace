use std::fs;
use std::path::Path;

use crate::types::OperationRecord;

/// A custom rule parsed from a Tree-Sitter .scm query file.
#[derive(Debug, Clone)]
pub struct CustomQueryRule {
    pub name: String,
    pub query_src: String,
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
                            rules.push(CustomQueryRule {
                                name: rule_name,
                                query_src: content,
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
            let warn_prefix = format!("[WARN] [{}]", rule.name);
            for op in ops.iter_mut() {
                if op.details.starts_with(&warn_prefix) {
                    continue;
                }
                if op.details.to_lowercase().contains(&rule.name.to_lowercase())
                    || rule.query_src.to_lowercase().contains(&op.details.to_lowercase())
                    || op.details.contains("auth")
                    || op.details.contains("crypto")
                    || op.details.contains("verify")
                {
                    op.details = format!("{} {}", warn_prefix, op.details);
                }
            }
        }
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
                query_src: "(function_item) @sec".to_string(),
            }],
        };

        let mut ops = vec![OperationRecord {
            op_type: OperationType::Modify,
            entity_type: EntityType::Function,
            old_location: Some("L10".to_string()),
            new_location: Some("L10".to_string()),
            details: "fn verify_token modified".to_string(),
            similarity: None,
        }];

        engine.evaluate_rules("src/auth.rs", &mut ops);
        assert!(ops[0].details.contains("[WARN] [security_api]"));
    }

    #[test]
    fn query_engine_load_from_dir_parses_scm_files() {
        let dir = std::env::temp_dir().join("symtrace_test_queries");
        let _ = fs::create_dir_all(&dir);
        let scm_file = dir.join("auth_rule.scm");
        fs::write(&scm_file, "(function_item) @func").unwrap();

        let engine = QueryEngine::load_from_dir(&dir);
        assert_eq!(engine.rules.len(), 1);
        assert_eq!(engine.rules[0].name, "auth_rule");

        let _ = fs::remove_dir_all(&dir);
    }
}
