//! Intra-Procedural Data-Flow & Variable Lineage Tracker
//!
//! Performs intra-procedural data-flow analysis on function AST bodies.
//! Identifies def-use chains for local variables to distinguish between
//! pure cosmetic renames (e.g. `i` -> `idx` with preserved def-use topology)
//! and functional mutations (modified return expressions or re-assignments).

use std::collections::{HashMap, HashSet};

use crate::node_identity;
use crate::types::AstNode;

/// The classified data-flow status for a modified function body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataFlowTag {
    /// Pure cosmetic rename of local variable(s) with identical def-use topology.
    CosmeticLocalRename,
    /// Data dependencies, variable computations, or return pathways were altered.
    DataFlowMutated,
    /// Only branch/loop control structures changed without modifying data flow.
    PureControlFlow,
    /// No detectable data-flow change.
    Unchanged,
}

impl std::fmt::Display for DataFlowTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CosmeticLocalRename => write!(f, "COSMETIC_LOCAL_RENAME"),
            Self::DataFlowMutated => write!(f, "DATA_FLOW_MUTATED"),
            Self::PureControlFlow => write!(f, "PURE_CONTROL_FLOW"),
            Self::Unchanged => write!(f, "UNCHANGED"),
        }
    }
}

/// Result of analyzing data flow between old and new function ASTs.
#[derive(Debug, Clone)]
pub struct DataFlowAnalysis {
    pub tag: DataFlowTag,
    pub declared_vars_old: Vec<String>,
    pub declared_vars_new: Vec<String>,
    pub return_expr_changed: bool,
    pub confidence: f64,
}

/// Analyze intra-procedural data flow between two versions of a function/method AST.
pub fn analyze_intra_procedural_data_flow(old_fn: &AstNode, new_fn: &AstNode) -> DataFlowAnalysis {
    let old_vars = extract_declared_variables(old_fn);
    let new_vars = extract_declared_variables(new_fn);

    let old_returns = extract_return_expressions(old_fn);
    let new_returns = extract_return_expressions(new_fn);

    let return_expr_changed = old_returns != new_returns;

    // Check if ASTs are identical in content
    if old_fn.content_hash == new_fn.content_hash {
        return DataFlowAnalysis {
            tag: DataFlowTag::Unchanged,
            declared_vars_old: old_vars,
            declared_vars_new: new_vars,
            return_expr_changed: false,
            confidence: 1.0,
        };
    }

    // Check for cosmetic local variable rename
    // Criteria:
    // 1. Same tree shape (structural_hash identical)
    // 2. Only identifiers changed in AST
    // 3. Same number of declared variables
    // 4. Return expressions structurally isomorphic
    let only_idents = node_identity::only_identifiers_changed(old_fn, new_fn);
    let same_shape = old_fn.structural_hash == new_fn.structural_hash;

    if same_shape && only_idents && old_vars.len() == new_vars.len() && !old_vars.is_empty() {
        let is_pure_rename = verify_def_use_isomorphism(old_fn, new_fn, &old_vars, &new_vars);
        if is_pure_rename {
            return DataFlowAnalysis {
                tag: DataFlowTag::CosmeticLocalRename,
                declared_vars_old: old_vars,
                declared_vars_new: new_vars,
                return_expr_changed,
                confidence: 0.95,
            };
        }
    }

    // Check for control flow alteration without variable mutation
    if old_vars == new_vars && !return_expr_changed && !only_idents {
        let old_ops = extract_assignment_operators(old_fn);
        let new_ops = extract_assignment_operators(new_fn);
        if old_ops == new_ops {
            return DataFlowAnalysis {
                tag: DataFlowTag::PureControlFlow,
                declared_vars_old: old_vars,
                declared_vars_new: new_vars,
                return_expr_changed,
                confidence: 0.85,
            };
        }
    }

    DataFlowAnalysis {
        tag: DataFlowTag::DataFlowMutated,
        declared_vars_old: old_vars,
        declared_vars_new: new_vars,
        return_expr_changed,
        confidence: 0.90,
    }
}

fn extract_declared_variables(node: &AstNode) -> Vec<String> {
    let mut vars = Vec::new();
    let mut visited = HashSet::new();

    fn collect(n: &AstNode, vars: &mut Vec<String>, visited: &mut HashSet<String>) {
        if matches!(
            n.kind.as_str(),
            "let_declaration"
                | "lexical_declaration"
                | "variable_declaration"
                | "variable_declarator"
                | "assignment_statement"
                | "short_var_decl"
                | "parameter"
                | "formal_parameter"
        ) {
            for child in &n.children {
                if matches!(child.kind.as_str(), "identifier" | "pattern" | "name") {
                    let name = child.text.trim().to_string();
                    if !name.is_empty() && visited.insert(name.clone()) {
                        vars.push(name);
                    }
                }
            }
        }
        for child in &n.children {
            collect(child, vars, visited);
        }
    }

    collect(node, &mut vars, &mut visited);
    vars
}

fn extract_return_expressions(node: &AstNode) -> Vec<String> {
    let mut returns = Vec::new();

    fn collect(n: &AstNode, returns: &mut Vec<String>) {
        if matches!(n.kind.as_str(), "return_statement" | "return_expression") {
            let expr_text: String = n
                .children
                .iter()
                .filter(|c| c.kind != "return" && c.kind != ";")
                .map(|c| c.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            returns.push(expr_text.trim().to_string());
        }
        for child in &n.children {
            collect(child, returns);
        }
    }

    collect(node, &mut returns);
    returns
}

fn extract_assignment_operators(node: &AstNode) -> Vec<String> {
    let mut ops = Vec::new();

    fn collect(n: &AstNode, ops: &mut Vec<String>) {
        if matches!(
            n.kind.as_str(),
            "assignment_expression" | "augmented_assignment_expression" | "binary_expression"
        ) {
            for child in &n.children {
                if matches!(
                    child.kind.as_str(),
                    "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "==" | "!=" | "<" | ">" | "<=" | ">="
                ) {
                    ops.push(child.kind.clone());
                }
            }
        }
        for child in &n.children {
            collect(child, ops);
        }
    }

    collect(node, &mut ops);
    ops
}

fn verify_def_use_isomorphism(
    old_fn: &AstNode,
    new_fn: &AstNode,
    old_vars: &[String],
    new_vars: &[String],
) -> bool {
    // Construct mapping old_var -> new_var based on declaration order
    let rename_map: HashMap<&str, &str> = old_vars
        .iter()
        .map(|s| s.as_str())
        .zip(new_vars.iter().map(|s| s.as_str()))
        .collect();

    // Verify pairwise leaves across both ASTs map 1:1
    let mut old_tokens = Vec::new();
    let mut new_tokens = Vec::new();

    fn collect_leaf_texts<'a>(n: &'a AstNode, out: &mut Vec<&'a str>) {
        if n.children.is_empty() {
            out.push(n.text.as_str());
        } else {
            for c in &n.children {
                collect_leaf_texts(c, out);
            }
        }
    }

    collect_leaf_texts(old_fn, &mut old_tokens);
    collect_leaf_texts(new_fn, &mut new_tokens);

    if old_tokens.len() != new_tokens.len() {
        return false;
    }

    for (ot, nt) in old_tokens.iter().zip(new_tokens.iter()) {
        if let Some(mapped_new) = rename_map.get(ot) {
            if *mapped_new != *nt {
                return false;
            }
        } else if *ot != *nt {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_var_fn(var_name: &str, is_return_changed: bool) -> AstNode {
        let return_val = if is_return_changed { "999" } else { var_name };
        let text = format!("fn compute() {{ let {} = 42; return {}; }}", var_name, return_val);
        let content_hash = *blake3::hash(text.as_bytes()).as_bytes();
        AstNode {
            id: 1,
            kind: "function_item".to_string(),
            start_byte: 0,
            end_byte: 50,
            start_row: 1,
            start_col: 0,
            end_row: 5,
            end_col: 1,
            text,
            structural_hash: [1u8; 32],
            content_hash,
            context_hash: [3u8; 32],
            identity_hash: [4u8; 32],
            children: vec![
                AstNode {
                    id: 2,
                    kind: "let_declaration".to_string(),
                    start_byte: 15,
                    end_byte: 30,
                    start_row: 2,
                    start_col: 4,
                    end_row: 2,
                    end_col: 18,
                    text: format!("let {} = 42;", var_name),
                    structural_hash: [10u8; 32],
                    content_hash: [11u8; 32],
                    context_hash: [12u8; 32],
                    identity_hash: [13u8; 32],
                    children: vec![
                        AstNode {
                            id: 3,
                            kind: "identifier".to_string(),
                            start_byte: 19,
                            end_byte: 19 + var_name.len(),
                            start_row: 2,
                            start_col: 8,
                            end_row: 2,
                            end_col: 8 + var_name.len(),
                            text: var_name.to_string(),
                            structural_hash: [0u8; 32],
                            content_hash: [0u8; 32],
                            context_hash: [0u8; 32],
                            identity_hash: [0u8; 32],
                            children: vec![],
                            is_named: true,
                        }
                    ],
                    is_named: true,
                },
                AstNode {
                    id: 4,
                    kind: "return_statement".to_string(),
                    start_byte: 32,
                    end_byte: 45,
                    start_row: 3,
                    start_col: 4,
                    end_row: 3,
                    end_col: 16,
                    text: format!("return {};", return_val),
                    structural_hash: [20u8; 32],
                    content_hash: [21u8; 32],
                    context_hash: [22u8; 32],
                    identity_hash: [23u8; 32],
                    children: vec![
                        AstNode {
                            id: 5,
                            kind: "identifier".to_string(),
                            start_byte: 39,
                            end_byte: 39 + return_val.len(),
                            start_row: 3,
                            start_col: 11,
                            end_row: 3,
                            end_col: 11 + return_val.len(),
                            text: return_val.to_string(),
                            structural_hash: [0u8; 32],
                            content_hash: [0u8; 32],
                            context_hash: [0u8; 32],
                            identity_hash: [0u8; 32],
                            children: vec![],
                            is_named: true,
                        }
                    ],
                    is_named: true,
                }
            ],
            is_named: true,
        }
    }

    #[test]
    fn test_cosmetic_local_rename_detection() {
        let old_fn = dummy_var_fn("i", false);
        let new_fn = dummy_var_fn("idx", false);

        let analysis = analyze_intra_procedural_data_flow(&old_fn, &new_fn);
        assert_eq!(analysis.tag, DataFlowTag::CosmeticLocalRename);
        assert_eq!(analysis.declared_vars_old, vec!["i"]);
        assert_eq!(analysis.declared_vars_new, vec!["idx"]);
    }

    #[test]
    fn test_data_flow_unchanged_identical() {
        let node = dummy_var_fn("counter", false);
        let analysis = analyze_intra_procedural_data_flow(&node, &node);
        assert_eq!(analysis.tag, DataFlowTag::Unchanged);
    }

    #[test]
    fn test_data_flow_mutated_return_expression() {
        let old_fn = dummy_var_fn("x", false);
        let new_fn = dummy_var_fn("x", true);
        let analysis = analyze_intra_procedural_data_flow(&old_fn, &new_fn);
        assert_eq!(analysis.tag, DataFlowTag::DataFlowMutated);
    }

    #[test]
    fn test_data_flow_variable_count_mismatch() {
        let old_fn = dummy_var_fn("a", false);
        let mut new_fn = dummy_var_fn("a", false);
        let extra_var = AstNode {
            id: 10,
            kind: "let_declaration".to_string(),
            start_byte: 55,
            end_byte: 70,
            start_row: 4,
            start_col: 4,
            end_row: 4,
            end_col: 18,
            text: "let b = 100;".to_string(),
            structural_hash: [10u8; 32],
            content_hash: [11u8; 32],
            context_hash: [12u8; 32],
            identity_hash: [13u8; 32],
            children: vec![
                AstNode {
                    id: 11,
                    kind: "identifier".to_string(),
                    start_byte: 59,
                    end_byte: 60,
                    start_row: 4,
                    start_col: 8,
                    end_row: 4,
                    end_col: 9,
                    text: "b".to_string(),
                    structural_hash: [0u8; 32],
                    content_hash: [0u8; 32],
                    context_hash: [0u8; 32],
                    identity_hash: [0u8; 32],
                    children: vec![],
                    is_named: true,
                }
            ],
            is_named: true,
        };
        new_fn.children.push(extra_var);
        new_fn.content_hash[0] = new_fn.content_hash[0].wrapping_add(1);
        let analysis = analyze_intra_procedural_data_flow(&old_fn, &new_fn);
        assert_eq!(analysis.tag, DataFlowTag::DataFlowMutated);
    }

    #[test]
    fn test_data_flow_display_strings() {
        assert_eq!(DataFlowTag::Unchanged.to_string(), "UNCHANGED");
        assert_eq!(DataFlowTag::CosmeticLocalRename.to_string(), "COSMETIC_LOCAL_RENAME");
        assert_eq!(DataFlowTag::DataFlowMutated.to_string(), "DATA_FLOW_MUTATED");
    }

    #[test]
    fn test_extract_declared_variables_empty() {
        let empty_node = AstNode {
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
        let vars = extract_declared_variables(&empty_node);
        assert!(vars.is_empty());
        let rets = extract_return_expressions(&empty_node);
        assert!(rets.is_empty());
        let ops = extract_assignment_operators(&empty_node);
        assert!(ops.is_empty());
    }

    #[test]
    fn test_verify_def_use_isomorphism_token_count_mismatch() {
        let n1 = dummy_var_fn("x", false);
        let mut n2 = dummy_var_fn("y", false);
        n2.children.clear();
        assert!(!verify_def_use_isomorphism(&n1, &n2, &["x".to_string()], &["y".to_string()]));
    }
}
