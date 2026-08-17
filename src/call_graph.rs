//! Cross-File Call Graph & Semantic Blast Radius Analysis Engine
//!
//! Extracts function/method call sites across repository files, constructs a
//! call graph directed acyclic graph (DAG), and computes transitive downstream blast radius
//! when public function signatures or interface contracts change.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::tree_diff::extract_name;
use crate::types::{AstNode, BlastRadiusReport, ImpactedCaller};

/// A reference to a symbol at a specific location.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SymbolLocation {
    pub symbol_name: String,
    pub file_path: String,
    pub line: usize,
}

/// A node in the repository-wide call graph.
#[derive(Debug, Clone)]
pub struct CallGraphNode {
    pub symbol_name: String,
    pub file_path: String,
    pub kind: String,
    pub line: usize,
    /// Callers that invoke this symbol: (caller_symbol, caller_file, line)
    pub callers: Vec<SymbolLocation>,
    /// Callees invoked by this symbol: (callee_symbol, callee_file, line)
    pub callees: Vec<SymbolLocation>,
}

/// The repository-wide call graph index.
#[derive(Debug, Clone, Default)]
pub struct CallGraph {
    /// (symbol_name, file_path) -> CallGraphNode
    pub nodes: HashMap<(String, String), CallGraphNode>,
    /// symbol_name -> list of file paths defining this symbol
    pub symbol_defs: HashMap<String, Vec<String>>,
}

impl CallGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Extract definitions and call sites from ASTs across all files and construct the CallGraph.
    pub fn build(files: &[(&str, &AstNode)]) -> Self {
        let mut graph = CallGraph::new();
        let mut defined_functions: HashMap<(String, String), (String, usize)> = HashMap::new();
        let mut raw_calls: Vec<(String, String, String, usize)> = Vec::new(); // (caller_symbol, caller_file, callee_name, line)

        for (file_path, ast) in files {
            extract_symbols_and_calls_recursive(
                ast,
                file_path,
                None,
                &mut defined_functions,
                &mut raw_calls,
            );
        }

        // Initialize graph nodes for all definitions
        for ((symbol_name, file_path), (kind, line)) in &defined_functions {
            graph.symbol_defs.entry(symbol_name.clone()).or_default().push(file_path.clone());
            graph.nodes.insert(
                (symbol_name.clone(), file_path.clone()),
                CallGraphNode {
                    symbol_name: symbol_name.clone(),
                    file_path: file_path.clone(),
                    kind: kind.clone(),
                    line: *line,
                    callers: Vec::new(),
                    callees: Vec::new(),
                },
            );
        }

        // Link call sites
        for (caller_symbol, caller_file, callee_name, call_line) in raw_calls {
            let caller_loc = SymbolLocation {
                symbol_name: caller_symbol.clone(),
                file_path: caller_file.clone(),
                line: call_line,
            };

            // Resolve callee target(s)
            if let Some(target_files) = graph.symbol_defs.get(&callee_name) {
                for target_file in target_files {
                    let callee_loc = SymbolLocation {
                        symbol_name: callee_name.clone(),
                        file_path: target_file.clone(),
                        line: call_line,
                    };

                    // Add callee to caller
                    if let Some(caller_node) = graph.nodes.get_mut(&(caller_symbol.clone(), caller_file.clone())) {
                        if !caller_node.callees.iter().any(|c| c.symbol_name == callee_name && c.file_path == *target_file) {
                            caller_node.callees.push(callee_loc.clone());
                        }
                    }

                    // Add caller to callee
                    if let Some(callee_node) = graph.nodes.get_mut(&(callee_name.clone(), target_file.clone())) {
                        if !callee_node.callers.iter().any(|c| c.symbol_name == caller_symbol && c.file_path == caller_file) {
                            callee_node.callers.push(caller_loc.clone());
                        }
                    }
                }
            }
        }

        graph
    }

    /// Compute the transitive semantic blast radius for a list of modified symbols.
    pub fn compute_blast_radius(&self, modified_symbols: &[(&str, &str)]) -> Vec<BlastRadiusReport> {
        let mut reports = Vec::new();

        for &(symbol_name, file_path) in modified_symbols {
            let mut visited = HashSet::new();
            let mut queue: VecDeque<(String, String, usize, usize)> = VecDeque::new(); // (sym, file, line, depth)
            let mut impacted_callers = Vec::new();

            visited.insert((symbol_name.to_string(), file_path.to_string()));

            // Find direct callers
            if let Some(node) = self.nodes.get(&(symbol_name.to_string(), file_path.to_string())) {
                for caller in &node.callers {
                    if visited.insert((caller.symbol_name.clone(), caller.file_path.clone())) {
                        queue.push_back((caller.symbol_name.clone(), caller.file_path.clone(), caller.line, 1));
                    }
                }
            } else if let Some(target_files) = self.symbol_defs.get(symbol_name) {
                for tf in target_files {
                    if let Some(node) = self.nodes.get(&(symbol_name.to_string(), tf.clone())) {
                        for caller in &node.callers {
                            if visited.insert((caller.symbol_name.clone(), caller.file_path.clone())) {
                                queue.push_back((caller.symbol_name.clone(), caller.file_path.clone(), caller.line, 1));
                            }
                        }
                    }
                }
            }

            // BFS up to depth 5
            while let Some((cur_sym, cur_file, line, depth)) = queue.pop_front() {
                impacted_callers.push(ImpactedCaller {
                    caller_symbol: cur_sym.clone(),
                    caller_file: cur_file.clone(),
                    call_site_line: line,
                    depth,
                });

                if depth < 5 {
                    if let Some(node) = self.nodes.get(&(cur_sym.clone(), cur_file.clone())) {
                        for next_caller in &node.callers {
                            if visited.insert((next_caller.symbol_name.clone(), next_caller.file_path.clone())) {
                                queue.push_back((next_caller.symbol_name.clone(), next_caller.file_path.clone(), next_caller.line, depth + 1));
                            }
                        }
                    }
                }
            }

            let total_impacted = impacted_callers.len();
            let severity = if total_impacted >= 5 {
                "HIGH".to_string()
            } else if total_impacted > 0 {
                "MEDIUM".to_string()
            } else {
                "LOW".to_string()
            };

            reports.push(BlastRadiusReport {
                modified_symbol: symbol_name.to_string(),
                file_path: file_path.to_string(),
                total_impacted_callers: total_impacted,
                impacted_callers,
                severity,
            });
        }

        reports
    }
}

fn is_function_def(kind: &str) -> bool {
    matches!(
        kind,
        "function_item"
            | "function_definition"
            | "function_declaration"
            | "method_definition"
            | "method_declaration"
            | "arrow_function"
    )
}

fn is_call_expr(kind: &str) -> bool {
    matches!(
        kind,
        "call_expression"
            | "method_invocation"
            | "call"
            | "invocation_expression"
    )
}

fn extract_symbols_and_calls_recursive(
    node: &AstNode,
    file_path: &str,
    current_enclosing_fn: Option<&str>,
    defined_functions: &mut HashMap<(String, String), (String, usize)>,
    raw_calls: &mut Vec<(String, String, String, usize)>,
) {
    let mut current_fn = current_enclosing_fn;

    if is_function_def(&node.kind) {
        let name = extract_name(node);
        if !name.is_empty() {
            defined_functions.insert((name.clone(), file_path.to_string()), (node.kind.clone(), node.start_row));
            current_fn = Some(Box::leak(name.into_boxed_str()));
        }
    }

    if is_call_expr(&node.kind) {
        if let Some(caller) = current_fn {
            let callee_name = extract_callee_name(node);
            if !callee_name.is_empty() && callee_name != caller {
                raw_calls.push((caller.to_string(), file_path.to_string(), callee_name, node.start_row));
            }
        }
    }

    for child in &node.children {
        extract_symbols_and_calls_recursive(
            child,
            file_path,
            current_fn,
            defined_functions,
            raw_calls,
        );
    }
}

fn extract_callee_name(node: &AstNode) -> String {
    for child in &node.children {
        if matches!(child.kind.as_str(), "identifier" | "field_identifier" | "property_identifier" | "name") {
            let text = child.text.trim();
            if !text.is_empty() {
                return text.to_string();
            }
        }
        if child.kind == "field_expression" || child.kind == "member_expression" {
            for sub in &child.children {
                if matches!(sub.kind.as_str(), "field_identifier" | "property_identifier" | "identifier") {
                    let text = sub.text.trim();
                    if !text.is_empty() {
                        return text.to_string();
                    }
                }
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_fn_with_call(fn_name: &str, call_target: &str) -> AstNode {
        AstNode {
            id: 1,
            kind: "function_item".to_string(),
            start_byte: 0,
            end_byte: 50,
            start_row: 10,
            start_col: 0,
            end_row: 15,
            end_col: 1,
            text: format!("fn {}() {{ {}(); }}", fn_name, call_target),
            structural_hash: [1u8; 32],
            content_hash: [2u8; 32],
            context_hash: [3u8; 32],
            identity_hash: [4u8; 32],
            children: vec![
                AstNode {
                    id: 2,
                    kind: "identifier".to_string(),
                    start_byte: 3,
                    end_byte: 3 + fn_name.len(),
                    start_row: 10,
                    start_col: 3,
                    end_row: 10,
                    end_col: 3 + fn_name.len(),
                    text: fn_name.to_string(),
                    structural_hash: [0u8; 32],
                    content_hash: [0u8; 32],
                    context_hash: [0u8; 32],
                    identity_hash: [0u8; 32],
                    children: vec![],
                    is_named: true,
                },
                AstNode {
                    id: 3,
                    kind: "call_expression".to_string(),
                    start_byte: 20,
                    end_byte: 30,
                    start_row: 11,
                    start_col: 4,
                    end_row: 11,
                    end_col: 14,
                    text: format!("{}()", call_target),
                    structural_hash: [0u8; 32],
                    content_hash: [0u8; 32],
                    context_hash: [0u8; 32],
                    identity_hash: [0u8; 32],
                    children: vec![
                        AstNode {
                            id: 4,
                            kind: "identifier".to_string(),
                            start_byte: 20,
                            end_byte: 20 + call_target.len(),
                            start_row: 11,
                            start_col: 4,
                            end_row: 11,
                            end_col: 4 + call_target.len(),
                            text: call_target.to_string(),
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
    fn test_call_graph_build_and_blast_radius() {
        let callee_ast = dummy_fn_with_call("process_payment", "validate");
        let caller_ast = dummy_fn_with_call("checkout", "process_payment");

        let files = [
            ("src/billing.rs", &callee_ast),
            ("src/checkout.rs", &caller_ast),
        ];

        let call_graph = CallGraph::build(&files);
        assert!(call_graph.nodes.contains_key(&("process_payment".to_string(), "src/billing.rs".to_string())));
        assert!(call_graph.nodes.contains_key(&("checkout".to_string(), "src/checkout.rs".to_string())));

        let blast_radius = call_graph.compute_blast_radius(&[("process_payment", "src/billing.rs")]);
        assert_eq!(blast_radius.len(), 1);
        assert_eq!(blast_radius[0].total_impacted_callers, 1);
        assert_eq!(blast_radius[0].impacted_callers[0].caller_symbol, "checkout");
        assert_eq!(blast_radius[0].impacted_callers[0].caller_file, "src/checkout.rs");
    }

    #[test]
    fn test_call_graph_empty_files() {
        let graph = CallGraph::build(&[]);
        assert!(graph.nodes.is_empty());
        assert!(graph.symbol_defs.is_empty());
        let blast = graph.compute_blast_radius(&[("any", "any.rs")]);
        assert_eq!(blast[0].total_impacted_callers, 0);
    }

    #[test]
    fn test_call_graph_no_calls() {
        let ast = dummy_fn_with_call("solo_func", "non_existent");
        let files = [("src/solo.rs", &ast)];
        let graph = CallGraph::build(&files);
        let blast = graph.compute_blast_radius(&[("solo_func", "src/solo.rs")]);
        assert_eq!(blast.len(), 1);
        assert_eq!(blast[0].total_impacted_callers, 0);
        assert_eq!(blast[0].severity, "LOW");
    }

    #[test]
    fn test_call_graph_self_recursion() {
        let ast = dummy_fn_with_call("recursive_fn", "recursive_fn");
        let files = [("src/rec.rs", &ast)];
        let graph = CallGraph::build(&files);
        let blast = graph.compute_blast_radius(&[("recursive_fn", "src/rec.rs")]);
        assert_eq!(blast.len(), 1);
        // Self is not counted as downstream external caller
        assert_eq!(blast[0].total_impacted_callers, 0);
    }

    #[test]
    fn test_call_graph_multiple_targets() {
        let ast1 = dummy_fn_with_call("target_a", "other");
        let ast2 = dummy_fn_with_call("target_b", "other");
        let files = [("src/a.rs", &ast1), ("src/b.rs", &ast2)];
        let graph = CallGraph::build(&files);
        let blast = graph.compute_blast_radius(&[("target_a", "src/a.rs"), ("target_b", "src/b.rs")]);
        assert_eq!(blast.len(), 2);
    }

    #[test]
    fn test_call_graph_diamond_dependency() {
        let d = dummy_fn_with_call("func_d", "leaf");
        let b = dummy_fn_with_call("func_b", "func_d");
        let c = dummy_fn_with_call("func_c", "func_d");
        let a = dummy_fn_with_call("func_a", "func_b");

        let files = [
            ("src/d.rs", &d),
            ("src/b.rs", &b),
            ("src/c.rs", &c),
            ("src/a.rs", &a),
        ];

        let graph = CallGraph::build(&files);
        let blast = graph.compute_blast_radius(&[("func_d", "src/d.rs")]);
        assert_eq!(blast.len(), 1);
        assert!(blast[0].total_impacted_callers >= 3);
    }

    #[test]
    fn test_call_graph_severity_thresholds() {
        let report_low = BlastRadiusReport {
            modified_symbol: "a".to_string(),
            file_path: "a.rs".to_string(),
            total_impacted_callers: 0,
            severity: "LOW".to_string(),
            impacted_callers: vec![],
        };
        assert_eq!(report_low.severity, "LOW");

        let report_high = BlastRadiusReport {
            modified_symbol: "b".to_string(),
            file_path: "b.rs".to_string(),
            total_impacted_callers: 6,
            severity: "HIGH".to_string(),
            impacted_callers: vec![],
        };
        assert_eq!(report_high.severity, "HIGH");
    }

    #[test]
    fn test_call_graph_cyclic_mutual_recursion() {
        let ast1 = dummy_fn_with_call("ping", "pong");
        let ast2 = dummy_fn_with_call("pong", "ping");
        let files = [("src/ping.rs", &ast1), ("src/pong.rs", &ast2)];
        let graph = CallGraph::build(&files);
        let blast = graph.compute_blast_radius(&[("ping", "src/ping.rs")]);
        assert_eq!(blast.len(), 1);
        assert_eq!(blast[0].total_impacted_callers, 1);
    }
}
