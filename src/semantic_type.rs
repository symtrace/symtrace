//! Type-Aware Structural Equivalence & Contract Violation Detector
//!
//! Performs lightweight cross-language AST type inference to recognize
//! type-safe refactoring patterns (Option -> Result, primitive widening)
//! and detects critical contract violations (removed null checks, deleted bounds checks,
//! stripped lock guards, omitted resource cleanup).

use std::collections::HashSet;

use crate::types::{AstNode, ContractViolation};

/// Scan AST modifications for deleted safety guards, removed bounds checks,
/// stripped lock guards, or omitted resource cleanup.
pub fn detect_contract_violations(
    file_path: &str,
    old_ast: &AstNode,
    new_ast: &AstNode,
) -> Vec<ContractViolation> {
    let mut violations = Vec::new();

    let old_guards = extract_safety_guards(old_ast);
    let new_guards = extract_safety_guards(new_ast);

    for (guard_kind, guard_text, line) in old_guards {
        if !new_guards.iter().any(|(k, t, _)| k == &guard_kind && t == &guard_text) {
            let (rule, message, severity) = match guard_kind.as_str() {
                "null_check" => (
                    "REMOVED_NULL_CHECK",
                    format!("Safety guard '{}' was removed, potential null-pointer dereference", guard_text),
                    "CRITICAL",
                ),
                "bounds_check" => (
                    "REMOVED_BOUNDS_CHECK",
                    format!("Bounds guard '{}' was removed, potential out-of-bounds access", guard_text),
                    "ERROR",
                ),
                "lock_guard" => (
                    "STRIPPED_LOCK_GUARD",
                    format!("Concurrency guard '{}' was removed, potential race condition", guard_text),
                    "CRITICAL",
                ),
                "resource_cleanup" => (
                    "OMITTED_RESOURCE_CLEANUP",
                    format!("Resource cleanup call '{}' was omitted, potential handle leak", guard_text),
                    "WARN",
                ),
                _ => (
                    "CONTRACT_VIOLATION",
                    format!("Safety contract guard '{}' was removed", guard_text),
                    "WARN",
                ),
            };

            violations.push(ContractViolation {
                file_path: file_path.to_string(),
                rule: rule.to_string(),
                message,
                line,
                severity: severity.to_string(),
            });
        }
    }

    violations
}

/// Detect type-safe refactoring patterns between old and new function ASTs.
pub fn detect_type_safe_refactors(old_node: &AstNode, new_node: &AstNode) -> Option<String> {
    let old_types = extract_type_annotations(old_node);
    let new_types = extract_type_annotations(new_node);

    if old_types.is_empty() || new_types.is_empty() {
        return None;
    }

    for (old_t, new_t) in old_types.iter().zip(new_types.iter()) {
        if old_t != new_t {
            if old_t.contains("Option") && new_t.contains("Result") {
                return Some(format!("Option to Result type-safe upgrade ({} -> {})", old_t, new_t));
            }
            if (old_t == "i32" && new_t == "i64") || (old_t == "int" && new_t == "int64_t") || (old_t == "u32" && new_t == "u64") {
                return Some(format!("Integer widening type refactor ({} -> {})", old_t, new_t));
            }
        }
    }

    None
}

fn get_node_text(node: &AstNode) -> String {
    if !node.text.is_empty() {
        return node.text.clone();
    }
    let mut tokens = Vec::new();
    fn collect(n: &AstNode, tokens: &mut Vec<String>) {
        if !n.text.is_empty() {
            tokens.push(n.text.clone());
        } else {
            for c in &n.children {
                collect(c, tokens);
            }
        }
    }
    collect(node, &mut tokens);
    tokens.join(" ")
}

fn extract_safety_guards(node: &AstNode) -> Vec<(String, String, usize)> {
    let mut guards = Vec::new();
    let mut visited = HashSet::new();

    fn collect(n: &AstNode, guards: &mut Vec<(String, String, usize)>, visited: &mut HashSet<String>) {
        // 1. Null / None checks
        if matches!(n.kind.as_str(), "binary_expression" | "comparison_operator" | "if_expression" | "if_statement") {
            let text = get_node_text(n);
            if text.contains("!= null") || text.contains("== null") || text.contains("is None") || text.contains("is not None") || text.contains("!= nil") || text.contains("== nil") || text.contains("null") {
                if visited.insert(text.clone()) {
                    guards.push(("null_check".to_string(), text.clone(), n.start_row));
                }
            }
            // 2. Bounds checks
            if (text.contains("< len") || text.contains("< size") || text.contains("< count") || text.contains("< length") || text.contains(">= 0")) && visited.insert(text.clone()) {
                guards.push(("bounds_check".to_string(), text.clone(), n.start_row));
            }
        }

        // 3. Concurrency / lock calls
        if matches!(n.kind.as_str(), "call_expression" | "method_invocation") {
            let text = get_node_text(n);
            let text_compact = text.replace(' ', "");
            if (text_compact.contains(".lock") || text_compact.contains(".acquire") || text_compact.contains("pthread_mutex_lock") || text_compact.contains("synchronized") || text_compact.contains("lock(")) && visited.insert(text.clone()) {
                guards.push(("lock_guard".to_string(), text.clone(), n.start_row));
            }
            // 4. Resource cleanup calls
            if (text_compact.contains(".close") || text_compact.contains(".dispose") || text_compact.contains("free") || text_compact.contains(".drop")) && visited.insert(text.clone()) {
                guards.push(("resource_cleanup".to_string(), text.clone(), n.start_row));
            }
        }

        for child in &n.children {
            collect(child, guards, visited);
        }
    }

    collect(node, &mut guards, &mut visited);
    guards
}

fn extract_type_annotations(node: &AstNode) -> Vec<String> {
    let mut types = Vec::new();

    fn collect(n: &AstNode, types: &mut Vec<String>) {
        if matches!(
            n.kind.as_str(),
            "type_identifier"
                | "primitive_type"
                | "generic_type"
                | "type_annotation"
                | "return_type"
        ) {
            let text = get_node_text(n);
            if !text.is_empty() && !types.contains(&text) {
                types.push(text);
            }
        }
        for child in &n.children {
            collect(child, types);
        }
    }

    collect(node, &mut types);
    types
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_guard_node(condition: &str) -> AstNode {
        AstNode {
            id: 1,
            kind: "binary_expression".to_string(),
            start_byte: 0,
            end_byte: 30,
            start_row: 12,
            start_col: 4,
            end_row: 12,
            end_col: 34,
            text: condition.to_string(),
            structural_hash: [1u8; 32],
            content_hash: [2u8; 32],
            context_hash: [3u8; 32],
            identity_hash: [4u8; 32],
            children: vec![],
            is_named: true,
        }
    }

    fn dummy_call_node(call: &str) -> AstNode {
        AstNode {
            id: 2,
            kind: "call_expression".to_string(),
            start_byte: 0,
            end_byte: 30,
            start_row: 15,
            start_col: 4,
            end_row: 15,
            end_col: 34,
            text: call.to_string(),
            structural_hash: [1u8; 32],
            content_hash: [2u8; 32],
            context_hash: [3u8; 32],
            identity_hash: [4u8; 32],
            children: vec![],
            is_named: true,
        }
    }

    fn dummy_type_node(typ: &str) -> AstNode {
        AstNode {
            id: 3,
            kind: "type_annotation".to_string(),
            start_byte: 0,
            end_byte: 20,
            start_row: 1,
            start_col: 0,
            end_row: 1,
            end_col: 20,
            text: typ.to_string(),
            structural_hash: [1u8; 32],
            content_hash: [2u8; 32],
            context_hash: [3u8; 32],
            identity_hash: [4u8; 32],
            children: vec![],
            is_named: true,
        }
    }

    #[test]
    fn test_detect_removed_null_check_violation() {
        let old_node = dummy_guard_node("ptr != null");
        let new_node = dummy_guard_node("true");

        let violations = detect_contract_violations("src/driver.c", &old_node, &new_node);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "REMOVED_NULL_CHECK");
        assert_eq!(violations[0].severity, "CRITICAL");
        assert_eq!(violations[0].line, 12);
    }

    #[test]
    fn test_detect_removed_bounds_check_violation() {
        let old_node = dummy_guard_node("idx < len");
        let new_node = dummy_guard_node("true");
        let violations = detect_contract_violations("src/array.rs", &old_node, &new_node);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "REMOVED_BOUNDS_CHECK");
        assert_eq!(violations[0].severity, "ERROR");
    }

    #[test]
    fn test_detect_stripped_lock_guard_violation() {
        let old_node = dummy_call_node("mutex.lock()");
        let new_node = dummy_call_node("process()");
        let violations = detect_contract_violations("src/sync.rs", &old_node, &new_node);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "STRIPPED_LOCK_GUARD");
        assert_eq!(violations[0].severity, "CRITICAL");
    }

    #[test]
    fn test_detect_omitted_resource_cleanup_violation() {
        let old_node = dummy_call_node("handle.close()");
        let new_node = dummy_call_node("return;");
        let violations = detect_contract_violations("src/io.rs", &old_node, &new_node);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].rule, "OMITTED_RESOURCE_CLEANUP");
        assert_eq!(violations[0].severity, "WARN");
    }

    #[test]
    fn test_detect_contract_violations_empty_when_guard_preserved() {
        let old_node = dummy_guard_node("ptr != null");
        let new_node = dummy_guard_node("ptr != null");
        let violations = detect_contract_violations("src/driver.c", &old_node, &new_node);
        assert!(violations.is_empty());
    }

    #[test]
    fn test_detect_type_safe_refactors_none_when_identical() {
        let old_node = dummy_type_node("i32");
        let new_node = dummy_type_node("i32");
        assert!(detect_type_safe_refactors(&old_node, &new_node).is_none());
    }

    #[test]
    fn test_detect_type_safe_refactors_primitive_widening() {
        let old_node = dummy_type_node("i32");
        let new_node = dummy_type_node("i64");
        let res = detect_type_safe_refactors(&old_node, &new_node);
        assert!(res.is_some());
        assert!(res.unwrap().contains("Integer widening"));
    }

    #[test]
    fn test_detect_type_safe_refactors_option_to_result() {
        let old_node = dummy_type_node("Option<User>");
        let new_node = dummy_type_node("Result<User, Error>");
        let res = detect_type_safe_refactors(&old_node, &new_node);
        assert!(res.is_some());
        assert!(res.unwrap().contains("Option to Result"));
    }
}
