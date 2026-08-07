mod ast_builder;
mod ast_cache;
mod cli;
mod commit_classification;
mod config;
mod git_layer;
mod incremental_parse;
mod language;
mod merge_driver;
mod node_identity;
mod output;
mod pager;
mod query_dsl;
mod refactor_detection;
mod semantic_similarity;
mod symbol_tracking;
mod tree_diff;
mod tui;
mod types;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::Parser;
use globset::Glob;
use rayon::prelude::*;

use ast_cache::{AstCache, CacheEntry};
use incremental_parse::TreeCache;
use types::*;

fn main() -> Result<()> {
    let args = cli::Args::parse();

    // ── 0. Check for subcommands (git-diff-driver mode) ───────────────
    match &args.command {
        Some(cli::Commands::GitDiffDriver {
            path,
            old_file,
            old_hex: _,
            old_mode: _,
            new_file,
            new_hex: _,
            new_mode: _,
        }) => {
            return run_git_diff_driver(path, old_file, new_file, &args);
        }
        Some(cli::Commands::MergeDriver {
            base_file,
            ours_file,
            theirs_file,
            display_path,
        }) => {
            let code = merge_driver::run_merge_driver(base_file, ours_file, theirs_file, display_path)?;
            std::process::exit(code);
        }
        _ => {}
    }

    let total_start = Instant::now();

    let (raw_repo, commit_a, commit_b) = resolve_cli_targets(&args);

    // ── Canonical path enforcement (prevents path spoofing) ──────────
    let canonical_repo = std::fs::canonicalize(&raw_repo)
        .with_context(|| format!("Failed to resolve repository path: '{}'", raw_repo))?;
    let canonical_repo = strip_unc_prefix(canonical_repo);

    if !canonical_repo.is_dir() {
        anyhow::bail!(
            "Repository path '{}' is not a directory",
            canonical_repo.display()
        );
    }

    let repo_path_str = canonical_repo.to_string_lossy().to_string();

    // ── Load configuration file (.symtracerc / symtrace.toml) ───────
    let user_config = config::Config::load(&canonical_repo, args.config.as_deref().map(Path::new));

    // Merge configuration values with CLI flags
    let logic_only = args.logic_only
        || user_config
            .default
            .as_ref()
            .and_then(|d| d.logic_only)
            .unwrap_or(false);
    let output_json = args.json
        || user_config
            .default
            .as_ref()
            .and_then(|d| d.json)
            .unwrap_or(false);
    let no_incremental = args.no_incremental
        || user_config
            .default
            .as_ref()
            .and_then(|d| d.no_incremental)
            .unwrap_or(false);
    let no_pager = args.no_pager
        || user_config
            .default
            .as_ref()
            .and_then(|d| d.no_pager)
            .unwrap_or(false);

    let color_setting = if args.color != "auto" {
        args.color.clone()
    } else {
        user_config
            .output
            .as_ref()
            .and_then(|o| o.color.clone())
            .unwrap_or_else(|| "auto".to_string())
    };

    output::configure_color(&color_setting);

    // ── External cache directory ──────────────────────────────────────
    let cache_dir = compute_cache_dir(&canonical_repo);
    let cache = Arc::new(AstCache::new(cache_dir));
    let tree_cache = Arc::new(TreeCache::new());

    // ── Parser resource limits ───────────────────────────────────────
    let limits = ParserLimits {
        max_file_size_bytes: user_config
            .limits
            .as_ref()
            .and_then(|l| l.max_file_size)
            .unwrap_or(args.max_file_size),
        max_ast_nodes: user_config
            .limits
            .as_ref()
            .and_then(|l| l.max_ast_nodes)
            .unwrap_or(args.max_ast_nodes),
        max_recursion_depth: user_config
            .limits
            .as_ref()
            .and_then(|l| l.max_recursion_depth)
            .unwrap_or(args.max_recursion_depth),
        parse_timeout_ms: user_config
            .limits
            .as_ref()
            .and_then(|l| l.parse_timeout_ms)
            .unwrap_or(args.parse_timeout_ms),
    };

    // ── 1. Git layer: discover changed files ─────────────────────────
    let changed_files = git_layer::get_changed_files(
        &repo_path_str,
        &commit_a,
        commit_b.as_deref(),
        args.staged,
    )?;

    // Apply --path glob filtering if specified
    let glob_matcher = if let Some(ref pattern) = args.path_glob {
        Some(Glob::new(pattern)?.compile_matcher())
    } else {
        None
    };

    let changed_files: Vec<_> = changed_files
        .into_iter()
        .filter(|fc| {
            if let Some(ref matcher) = glob_matcher {
                matcher.is_match(fc.display_path())
            } else {
                true
            }
        })
        .collect();

    // ── 2. Parse ASTs for each changed file (parallel + cached) ──────
    let parse_start = Instant::now();

    let work_items: Vec<_> = changed_files
        .iter()
        .filter_map(|fc| {
            let lang = language::detect_language(&fc.path)?;
            Some((fc, lang))
        })
        .collect();

    let parsed_results: Vec<_> = work_items
        .par_iter()
        .map(|(file_change, lang)| {
            if ast_cache::blobs_are_identical(
                file_change.old_blob_hash.as_deref(),
                file_change.new_blob_hash.as_deref(),
            ) {
                return (
                    file_change.path.clone(),
                    None,
                    None,
                    0u64,
                    true,
                    0u64,
                    false,
                );
            }

            let (ast_a, nodes_a, tree_a) = parse_or_cached_with_tree(
                &cache,
                &tree_cache,
                file_change.old_content.as_deref(),
                file_change.old_blob_hash.as_deref(),
                *lang,
                logic_only,
                &limits,
            );

            let (ast_b, nodes_b, nodes_reused, was_incremental) = {
                let try_incremental = !no_incremental
                    && tree_a.is_some()
                    && ast_a.is_some()
                    && file_change.old_content.is_some()
                    && file_change.new_content.is_some();

                if try_incremental {
                    let old_tree = tree_a.as_ref().unwrap();
                    let old_ast = ast_a.as_ref().unwrap();
                    let old_content = file_change.old_content.as_deref().unwrap();
                    let new_content = file_change.new_content.as_deref().unwrap();

                    match ast_builder::parse_content_incremental(
                        new_content,
                        old_content,
                        old_tree,
                        old_ast,
                        *lang,
                        logic_only,
                        &limits,
                    ) {
                        Ok((ast, new_tree, reused)) => {
                            let nc = tree_diff::count_nodes(&ast);
                            if let Some(bh) = file_change.new_blob_hash.as_deref() {
                                cache.put_by_oid(
                                    bh,
                                    logic_only,
                                    limits.compute_limits_hash(),
                                    CacheEntry {
                                        ast: ast.clone(),
                                        node_count: nc,
                                    },
                                );
                                tree_cache.put(bh.to_string(), new_tree);
                            }
                            (Some(ast), nc, reused, true)
                        }
                        Err(_e) => {
                            let (ast, nodes, _tree) = parse_or_cached_with_tree(
                                &cache,
                                &tree_cache,
                                file_change.new_content.as_deref(),
                                file_change.new_blob_hash.as_deref(),
                                *lang,
                                logic_only,
                                &limits,
                            );
                            (ast, nodes, 0, false)
                        }
                    }
                } else {
                    let (ast, nodes, _tree) = parse_or_cached_with_tree(
                        &cache,
                        &tree_cache,
                        file_change.new_content.as_deref(),
                        file_change.new_blob_hash.as_deref(),
                        *lang,
                        logic_only,
                        &limits,
                    );
                    (ast, nodes, 0, false)
                }
            };

            (
                file_change.path.clone(),
                ast_a,
                ast_b,
                nodes_a + nodes_b,
                false,
                nodes_reused,
                was_incremental,
            )
        })
        .collect();

    let mut parsed_pairs: Vec<(String, Option<AstNode>, Option<AstNode>)> = Vec::new();
    let mut total_nodes: u64 = 0;
    let mut files_processed: usize = 0;
    let mut files_skipped_blob: usize = 0;
    let mut total_nodes_reused: u64 = 0;
    let mut total_incremental_parses: usize = 0;

    for (path, ast_a, ast_b, nodes, skipped, reused, was_inc) in parsed_results {
        if skipped {
            files_skipped_blob += 1;
            continue;
        }
        total_nodes += nodes;
        total_nodes_reused += reused;
        if was_inc {
            total_incremental_parses += 1;
        }
        files_processed += 1;
        parsed_pairs.push((path, ast_a, ast_b));
    }
    let parse_time = parse_start.elapsed();

    // ── 3. Compute semantic diff per file (parallel) ─────────────────
    let diff_start = Instant::now();

    let mut file_diffs: Vec<FileDiff> = parsed_pairs
        .par_iter()
        .map(|(path, ast_a, ast_b)| {
            let operations = tree_diff::compute_diff(ast_a.as_ref(), ast_b.as_ref(), logic_only);
            let refactor_patterns =
                refactor_detection::detect_patterns(&operations, ast_a.as_ref(), ast_b.as_ref());

            FileDiff {
                file_path: path.clone(),
                operations,
                refactor_patterns,
            }
        })
        .collect();

    let diff_time = diff_start.elapsed();

    // ── 4. Cross-file symbol tracking ─────────────────────────────────
    let cross_file_tracking = symbol_tracking::track_cross_file_symbols(&parsed_pairs);

    // ── 5. Build summary ─────────────────────────────────────────────
    let summary = build_summary(&file_diffs);

    // ── 6. Commit classification ─────────────────────────────────────
    let logic_only_no_changes = if !logic_only {
        let files_with_ops: Vec<_> = parsed_pairs
            .iter()
            .zip(file_diffs.iter())
            .filter(|(_, fd)| !fd.operations.is_empty())
            .collect();

        let any_logic_ops = if files_with_ops.is_empty() {
            false
        } else {
            files_with_ops.par_iter().any(|((_, ast_a, ast_b), _)| {
                let logic_ops = tree_diff::compute_diff(ast_a.as_ref(), ast_b.as_ref(), true);
                !logic_ops.is_empty()
            })
        };
        !any_logic_ops && !file_diffs.is_empty()
    } else {
        file_diffs.iter().all(|fd| fd.operations.is_empty())
    };

    // Evaluate custom Tree-Sitter .scm rules if present
    let queries_dir = Path::new(&repo_path_str).join(".symtrace").join("queries");
    let query_engine = query_dsl::QueryEngine::load_from_dir(&queries_dir);
    for file_diff in &mut file_diffs {
        query_engine.evaluate_rules(&file_diff.file_path, &mut file_diff.operations);
    }

    let commit_classification =
        commit_classification::classify_commit(&file_diffs, &summary, logic_only_no_changes);

    let total_time = total_start.elapsed();
    let (cache_mem, cache_disk) = cache.stats();

    let diff_output = DiffOutput {
        repository: repo_path_str.clone(),
        commit_a: commit_a.clone(),
        commit_b: commit_b
            .clone()
            .unwrap_or_else(|| "WORKING".to_string()),
        files: file_diffs,
        summary,
        cross_file_tracking: Some(cross_file_tracking),
        commit_classification: Some(commit_classification),
        performance: PerformanceMetrics {
            total_files_processed: files_processed,
            total_nodes_compared: total_nodes,
            parse_time_ms: parse_time.as_secs_f64() * 1000.0,
            diff_time_ms: diff_time.as_secs_f64() * 1000.0,
            total_time_ms: total_time.as_secs_f64() * 1000.0,
            incremental_parses: total_incremental_parses,
            nodes_reused: total_nodes_reused,
        },
    };

    // ── 7. Output with shell pager support ───────────────────────────
    let mut pager = pager::Pager::setup(no_pager);
    let fmt = output::OutputFormat::parse(&args.format)?;

    let formatted_output = if args.stat {
        output::format_stat(&diff_output)
    } else if args.name_only {
        output::format_name_only(&diff_output)
    } else if output_json || fmt == output::OutputFormat::Json {
        output::format_json(&diff_output)?
    } else {
        match fmt {
            output::OutputFormat::Ansi => output::format_cli(&diff_output),
            output::OutputFormat::Json => output::format_json(&diff_output)?,
            output::OutputFormat::Jsonl => output::format_jsonl(&diff_output)?,
            output::OutputFormat::Markdown => output::format_markdown(&diff_output),
            output::OutputFormat::Html => output::format_html(&diff_output),
            output::OutputFormat::Sarif => output::format_sarif(&diff_output)?,
        }
    };

    if matches!(&args.command, Some(cli::Commands::Tui { .. })) {
        return tui::run_tui_inspector(&diff_output);
    }

    if fmt == output::OutputFormat::Html && !args.stat && !args.name_only {
        let html_content = output::format_html(&diff_output);
        let report_path = "symtrace_report.html";
        std::fs::write(report_path, &html_content)
            .with_context(|| format!("Failed to write HTML report to '{}'", report_path))?;

        println!("[SUCCESS] Generated interactive HTML report: {}", report_path);
        println!("[INFO] Opening report in default web browser...");

        #[cfg(target_os = "windows")]
        let _ = std::process::Command::new("cmd").args(["/C", "start", report_path]).spawn();
        #[cfg(target_os = "macos")]
        let _ = std::process::Command::new("open").arg(report_path).spawn();
        #[cfg(target_os = "linux")]
        let _ = std::process::Command::new("xdg-open").arg(report_path).spawn();

        return Ok(());
    }

    pager.print_output(&formatted_output)?;

    if !output_json && !args.stat && !args.name_only && fmt == output::OutputFormat::Ansi {
        let mut diag = String::new();
        if files_skipped_blob > 0 {
            diag.push_str(&format!(
                "  [FAST] Blob hash short-circuit: {} file(s) skipped (unchanged content)\n",
                files_skipped_blob
            ));
        }
        diag.push_str(&format!(
            "  [CACHE] AST cache: {} in-memory, {} on-disk entries\n",
            cache_mem, cache_disk
        ));
        diag.push_str(&format!(
            "  [TREE] Tree cache: {} in-memory entries\n",
            tree_cache.len()
        ));
        if total_incremental_parses > 0 {
            diag.push_str(&format!(
                "  [REUSE] Incremental parsing: {} file(s), {} nodes reused\n",
                total_incremental_parses, total_nodes_reused
            ));
        }
        pager.print_output(&diag)?;
    }

    pager.finish();

    if args.check {
        let has_structural_ops = diff_output.files.iter().any(|f| !f.operations.is_empty());
        if has_structural_ops {
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Run native Git diff driver mode for a single file comparison.
fn run_git_diff_driver(path: &str, old_file: &str, new_file: &str, args: &cli::Args) -> Result<()> {
    output::configure_color(&args.color);

    let lang = match language::detect_language(path) {
        Some(l) => l,
        None => {
            // Unsupported language fallback
            eprintln!("symtrace: unsupported language for file '{}'", path);
            return Ok(());
        }
    };

    let old_content = read_file_content_or_none(old_file);
    let new_content = read_file_content_or_none(new_file);

    let limits = ParserLimits {
        max_file_size_bytes: args.max_file_size,
        max_ast_nodes: args.max_ast_nodes,
        max_recursion_depth: args.max_recursion_depth,
        parse_timeout_ms: args.parse_timeout_ms,
    };

    let ast_a = old_content
        .as_deref()
        .and_then(|c| ast_builder::parse_content(c, lang, args.logic_only, &limits).ok());
    let ast_b = new_content
        .as_deref()
        .and_then(|c| ast_builder::parse_content(c, lang, args.logic_only, &limits).ok());

    let operations = tree_diff::compute_diff(ast_a.as_ref(), ast_b.as_ref(), args.logic_only);
    let refactor_patterns =
        refactor_detection::detect_patterns(&operations, ast_a.as_ref(), ast_b.as_ref());

    let file_diff = FileDiff {
        file_path: path.to_string(),
        operations,
        refactor_patterns,
    };

    let summary = build_summary(std::slice::from_ref(&file_diff));
    let diff_output = DiffOutput {
        repository: "git-diff-driver".to_string(),
        commit_a: old_file.to_string(),
        commit_b: new_file.to_string(),
        files: vec![file_diff],
        summary,
        cross_file_tracking: None,
        commit_classification: None,
        performance: PerformanceMetrics {
            total_files_processed: 1,
            total_nodes_compared: 0,
            parse_time_ms: 0.0,
            diff_time_ms: 0.0,
            total_time_ms: 0.0,
            incremental_parses: 0,
            nodes_reused: 0,
        },
    };

    let mut pager = pager::Pager::setup(args.no_pager);
    if args.json {
        pager.print_output(&output::format_json(&diff_output)?)?;
    } else {
        pager.print_output(&output::format_cli(&diff_output))?;
    }
    pager.finish();

    Ok(())
}

fn read_file_content_or_none(path: &str) -> Option<String> {
    if path == "." || path == "/dev/null" || path.is_empty() {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

fn parse_or_cached_with_tree(
    cache: &AstCache,
    tree_cache: &TreeCache,
    content: Option<&str>,
    blob_hash: Option<&str>,
    lang: SupportedLanguage,
    logic_only: bool,
    limits: &ParserLimits,
) -> (Option<AstNode>, u64, Option<tree_sitter::Tree>) {
    // ── 1. Zero-Copy Cache Lookup using blob_hash (OID) ─────────────
    if let Some(bh) = blob_hash {
        let limits_hash = limits.compute_limits_hash();
        if let Some(entry) = cache.get_by_oid(bh, logic_only, limits_hash) {
            let tree = tree_cache.get(bh);
            return (Some(entry.ast), entry.node_count, tree);
        }
    }

    // ── 2. Fallback to parsing content if content is present ─────────
    let content = match content {
        Some(c) => c,
        None => return (None, 0, None),
    };

    match ast_builder::parse_content_with_tree(content, lang, logic_only, limits) {
        Ok((ast, tree)) => {
            let node_count = tree_diff::count_nodes(&ast);
            if let Some(bh) = blob_hash {
                let limits_hash = limits.compute_limits_hash();
                cache.put_by_oid(
                    bh,
                    logic_only,
                    limits_hash,
                    CacheEntry {
                        ast: ast.clone(),
                        node_count,
                    },
                );
                tree_cache.put(bh.to_string(), tree.clone());
            }
            (Some(ast), node_count, Some(tree))
        }
        Err(e) => {
            eprintln!("  warning: parse failed: {}", e);
            (None, 0, None)
        }
    }
}

fn build_summary(file_diffs: &[FileDiff]) -> DiffSummary {
    let mut summary = DiffSummary {
        total_files: file_diffs.len(),
        moves: 0,
        renames: 0,
        inserts: 0,
        deletes: 0,
        modifications: 0,
    };

    for fd in file_diffs {
        for op in &fd.operations {
            match op.op_type {
                OperationType::Move => summary.moves += 1,
                OperationType::Rename => summary.renames += 1,
                OperationType::Insert => summary.inserts += 1,
                OperationType::Delete => summary.deletes += 1,
                OperationType::Modify => summary.modifications += 1,
            }
        }
    }

    summary
}

fn cache_base_dir() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg));
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(local) = std::env::var("LOCALAPPDATA") {
            if !local.is_empty() {
                return Some(PathBuf::from(local));
            }
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return Some(PathBuf::from(home).join(".cache"));
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(profile) = std::env::var("USERPROFILE") {
            if !profile.is_empty() {
                return Some(PathBuf::from(profile).join(".cache"));
            }
        }
    }

    None
}

fn compute_cache_dir(canonical_repo: &Path) -> Option<PathBuf> {
    let path_str = canonical_repo.to_string_lossy();
    let repo_hash = blake3::hash(path_str.as_bytes());
    let hex = repo_hash.to_hex();

    let base = cache_base_dir()?;
    Some(base.join("symtrace").join(hex.as_str()))
}

fn strip_unc_prefix(path: PathBuf) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let s = path.to_string_lossy();
        if let Some(stripped) = s.strip_prefix(r"\\?\") {
            return PathBuf::from(stripped);
        }
    }
    path
}

fn resolve_cli_targets(args: &cli::Args) -> (String, String, Option<String>) {
    if let Some(cli::Commands::Tui { commit_a, commit_b }) = &args.command {
        let repo = args.repo_flag.clone().unwrap_or_else(|| ".".to_string());
        return (repo, commit_a.clone(), commit_b.clone());
    }

    if let Some(ref repo_path) = args.repo_flag {
        let commit_a = args.arg1.clone().unwrap_or_else(|| "HEAD~1".to_string());
        let commit_b = args.arg2.clone();
        return (repo_path.clone(), commit_a, commit_b);
    }

    let raw_args: Vec<String> = [&args.arg1, &args.arg2, &args.arg3]
        .iter()
        .filter_map(|a| a.as_ref().cloned())
        .collect();

    match raw_args.as_slice() {
        [a1, a2, a3, ..] => (a1.clone(), a2.clone(), Some(a3.clone())),
        [a1, a2] => {
            if Path::new(a1).is_dir() {
                (a1.clone(), a2.clone(), None)
            } else {
                (".".to_string(), a1.clone(), Some(a2.clone()))
            }
        }
        [a1] => {
            if Path::new(a1).is_dir() {
                (a1.clone(), "HEAD~1".to_string(), None)
            } else {
                (".".to_string(), a1.clone(), None)
            }
        }
        _ => (".".to_string(), "HEAD~1".to_string(), None),
    }
}
