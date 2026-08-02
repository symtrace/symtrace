mod ast_builder;
mod ast_cache;
mod cli;
mod commit_classification;
mod config;
mod git_layer;
mod incremental_parse;
mod language;
mod node_identity;
mod output;
mod pager;
mod refactor_detection;
mod semantic_similarity;
mod symbol_tracking;
mod tree_diff;
mod types;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::Parser;
use globset::Glob;
use rayon::prelude::*;

use ast_cache::{AstCache, CacheEntry, CacheKey};
use incremental_parse::TreeCache;
use types::*;

fn main() -> Result<()> {
    let args = cli::Args::parse();

    // ── 0. Check for subcommands (git-diff-driver mode) ───────────────
    if let Some(cli::Commands::GitDiffDriver {
        path,
        old_file,
        old_hex: _,
        old_mode: _,
        new_file,
        new_hex: _,
        new_mode: _,
    }) = &args.command
    {
        return run_git_diff_driver(path, old_file, new_file, &args);
    }

    let total_start = Instant::now();

    // ── Canonical path enforcement (prevents path spoofing) ──────────
    let canonical_repo = std::fs::canonicalize(&args.repo_path)
        .with_context(|| format!("Failed to resolve repository path: '{}'", args.repo_path))?;
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
        &args.commit_a,
        args.commit_b.as_deref(),
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
                                let key = CacheKey {
                                    blob_hash: bh.to_string(),
                                    logic_only,
                                    limits_hash: limits.compute_limits_hash(),
                                };
                                cache.put(
                                    key,
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

    let file_diffs: Vec<FileDiff> = parsed_pairs
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

    let commit_classification =
        commit_classification::classify_commit(&file_diffs, &summary, logic_only_no_changes);

    let total_time = total_start.elapsed();
    let (cache_mem, cache_disk) = cache.stats();

    let diff_output = DiffOutput {
        repository: repo_path_str.clone(),
        commit_a: args.commit_a.clone(),
        commit_b: args
            .commit_b
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

    if output_json {
        let json_str = output::format_json(&diff_output)?;
        pager.print_output(&json_str)?;
    } else {
        let cli_str = output::format_cli(&diff_output);
        pager.print_output(&cli_str)?;

        let mut diag = String::new();
        if files_skipped_blob > 0 {
            diag.push_str(&format!(
                "  ⚡ Blob hash short-circuit: {} file(s) skipped (unchanged content)\n",
                files_skipped_blob
            ));
        }
        diag.push_str(&format!(
            "  📦 AST cache: {} in-memory, {} on-disk entries\n",
            cache_mem, cache_disk
        ));
        diag.push_str(&format!(
            "  🌲 Tree cache: {} in-memory entries\n",
            tree_cache.len()
        ));
        if total_incremental_parses > 0 {
            diag.push_str(&format!(
                "  🔄 Incremental parsing: {} file(s), {} nodes reused\n",
                total_incremental_parses, total_nodes_reused
            ));
        }
        pager.print_output(&diag)?;
    }

    pager.finish();
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
    let content = match content {
        Some(c) => c,
        None => return (None, 0, None),
    };

    if let Some(bh) = blob_hash {
        let key = CacheKey {
            blob_hash: bh.to_string(),
            logic_only,
            limits_hash: limits.compute_limits_hash(),
        };
        if let Some(entry) = cache.get(&key) {
            let tree = tree_cache.get(bh);
            return (Some(entry.ast), entry.node_count, tree);
        }
    }

    match ast_builder::parse_content_with_tree(content, lang, logic_only, limits) {
        Ok((ast, tree)) => {
            let node_count = tree_diff::count_nodes(&ast);
            if let Some(bh) = blob_hash {
                let key = CacheKey {
                    blob_hash: bh.to_string(),
                    logic_only,
                    limits_hash: limits.compute_limits_hash(),
                };
                cache.put(
                    key,
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
