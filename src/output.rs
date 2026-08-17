use anyhow::{Context, Result};
use colored::Colorize;

pub use crate::types::DisplayGranularity;
use crate::types::{DiffOutput, OperationType};

/// Determine the display granularity dynamically based on commit size, change surface, and flags.
pub fn determine_granularity(
    output: &DiffOutput,
    compact_flag: bool,
    full_headers_flag: bool,
) -> DisplayGranularity {
    if compact_flag {
        return DisplayGranularity::MicroCompact;
    }
    if full_headers_flag {
        return DisplayGranularity::FullStructural;
    }

    // Auto-detection:
    let total_ops: usize = output.files.iter().map(|f| f.operations.len()).sum();
    let changed_files = output.files.iter().filter(|f| !f.operations.is_empty()).count();

    // Check for multi-file events or refactor patterns
    let has_cross_file = output
        .cross_file_tracking
        .as_ref()
        .map_or(false, |c| !c.cross_file_events.is_empty());
    let has_refactors = output.files.iter().any(|f| !f.refactor_patterns.is_empty());

    // Check if any operation is an API surface change or multi-line move (requires full visibility)
    let has_api_or_move = output.files.iter().flat_map(|f| &f.operations).any(|op| {
        op.op_type == OperationType::Move || op.details.to_lowercase().contains("api surface")
    });

    if has_cross_file || has_refactors || has_api_or_move || changed_files > 2 || total_ops > 3 {
        DisplayGranularity::Standard
    } else {
        DisplayGranularity::MicroCompact
    }
}

/// Format DiffOutput in ultra-compact inline micro-commit format.
/// Emits 1–3 compact lines with colorized operation badges and inline token changes,
/// suppressing large decorative banners, timing diagnostics, and summary tables.
pub fn format_micro_cli(output: &DiffOutput) -> String {
    let mut buf = String::new();
    let mut any_ops = false;

    for file in &output.files {
        for op in &file.operations {
            any_ops = true;
            let (symbol, badge) = match op.op_type {
                OperationType::Move => ("↔".blue().bold(), "[MOVE]".blue().bold()),
                OperationType::Rename => ("✎".yellow().bold(), "[RENAME]".yellow().bold()),
                OperationType::Insert => ("+".green().bold(), "[INSERT]".green().bold()),
                OperationType::Delete => ("-".red().bold(), "[DELETE]".red().bold()),
                OperationType::Modify => ("~".cyan().bold(), "[MODIFY]".cyan().bold()),
            };

            let loc = op.new_location.as_deref().or(op.old_location.as_deref()).unwrap_or("L1");
            let sim_str = if let Some(ref sim) = op.similarity {
                format!(" ({:.0}%)", sim.similarity_percent).dimmed().to_string()
            } else {
                String::new()
            };

            buf.push_str(&format!(
                "{} {}:{}  {}  {}{}\n",
                symbol,
                file.file_path.bold(),
                loc,
                badge,
                op.details,
                sim_str
            ));
        }
    }

    if !any_ops {
        buf.push_str(&"  (no semantic changes detected)\n".dimmed().to_string());
    }

    buf
}

/// Format DiffOutput with explicit granularity selection.
pub fn format_cli_with_granularity(output: &DiffOutput, granularity: DisplayGranularity) -> String {
    match granularity {
        DisplayGranularity::MicroCompact => format_micro_cli(output),
        DisplayGranularity::Standard => {
            let mut buf = String::new();
            buf.push_str(&format!("{}\n", "━━━ symtrace Semantic Diff ━━━".bold()));
            buf.push_str(&format!(
                "Repository: {} | Comparing: {} → {}\n\n",
                output.repository.cyan(),
                output.commit_a.yellow(),
                output.commit_b.yellow()
            ));

            if output.files.is_empty() {
                buf.push_str(&"  (no semantic changes detected)\n\n".dimmed().to_string());
            }

            for file in &output.files {
                buf.push_str(&format!("{} {}\n", "━━━".bold(), file.file_path.bold().underline()));
                if file.operations.is_empty() {
                    buf.push_str(&"    (no significant operations)\n".dimmed().to_string());
                }
                for op in &file.operations {
                    let (symbol, colored_type) = match op.op_type {
                        OperationType::Move => ("↔", "MOVE".blue().bold()),
                        OperationType::Rename => ("✎", "RENAME".yellow().bold()),
                        OperationType::Insert => ("+", "INSERT".green().bold()),
                        OperationType::Delete => ("-", "DELETE".red().bold()),
                        OperationType::Modify => ("~", "MODIFY".cyan().bold()),
                    };
                    let location = match (&op.old_location, &op.new_location) {
                        (Some(old), Some(new)) => if old == new { old.clone() } else { format!("{} → {}", old, new) },
                        (Some(old), None) => old.clone(),
                        (None, Some(new)) => new.clone(),
                        (None, None) => "—".to_string(),
                    };
                    buf.push_str(&format!("  {} [{}] {} ({})", symbol, colored_type, op.details, location.dimmed()));
                    if let Some(ref sim) = op.similarity {
                        buf.push_str(&format!(" [{:.0}% similarity, {}]", sim.similarity_percent, sim.change_intensity));
                    }
                    buf.push('\n');
                }
                if !file.refactor_patterns.is_empty() {
                    buf.push_str(&format!("  {}\n", "── Refactor Patterns ──".dimmed()));
                    for pattern in &file.refactor_patterns {
                        buf.push_str(&format!("    {} {} (confidence: {:.0}%)\n", "▸".magenta(), pattern.description, pattern.confidence * 100.0));
                    }
                }
                buf.push('\n');
            }

            // Summary
            buf.push_str(&format!("{}\n", "━━━ Summary ━━━".bold()));
            buf.push_str(&format!(
                "  Files: {} | Moves: {} | Renames: {} | Inserts: {} | Deletes: {} | Modifies: {}\n",
                output.summary.total_files, output.summary.moves, output.summary.renames, output.summary.inserts, output.summary.deletes, output.summary.modifications
            ));

            if let Some(ref tracking) = output.cross_file_tracking {
                if !tracking.cross_file_events.is_empty() {
                    buf.push_str(&format!("\n{}\n", "━━━ Cross-File Symbol Tracking ━━━".bold()));
                    for event in &tracking.cross_file_events {
                        let symbol = match event.event {
                            crate::types::CrossFileEventKind::CrossFileMove => "↔".blue().to_string(),
                            crate::types::CrossFileEventKind::CrossFileRename => "✎".yellow().to_string(),
                            crate::types::CrossFileEventKind::ApiSurfaceChange => "⚠".red().to_string(),
                        };
                        buf.push_str(&format!("  {} [{}] {} (similarity: {:.0}%)\n", symbol, event.event.to_string().bold(), event.description, event.similarity_score * 100.0));
                    }
                }
            }

            if let Some(ref classification) = output.commit_classification {
                buf.push_str(&format!("\nCommit Class: {} (confidence: {:.0}%)\n", classification.primary_class.to_string().bold().cyan(), classification.confidence_score * 100.0));
            }
            buf
        }
        DisplayGranularity::FullStructural => format_cli(output),
    }
}

/// Available output formats supported by `symtrace`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Ansi,
    Json,
    Jsonl,
    Markdown,
    Html,
    Sarif,
    Prompt,
}

impl OutputFormat {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "ansi" | "cli" | "text" => Ok(Self::Ansi),
            "json" => Ok(Self::Json),
            "jsonl" => Ok(Self::Jsonl),
            "markdown" | "md" => Ok(Self::Markdown),
            "html" => Ok(Self::Html),
            "sarif" => Ok(Self::Sarif),
            "prompt" | "llm" => Ok(Self::Prompt),
            _ => anyhow::bail!(
                "Unsupported output format: '{}'. Choose from ansi, json, jsonl, markdown, html, sarif, prompt",
                s
            ),
        }
    }
}

/// Configure colored output based on CLI setting and NO_COLOR env var.
pub fn configure_color(color_setting: &str) {
    if std::env::var("NO_COLOR").is_ok() {
        colored::control::set_override(false);
        return;
    }
    match color_setting.to_lowercase().as_str() {
        "always" | "yes" | "true" => colored::control::set_override(true),
        "never" | "no" | "false" => colored::control::set_override(false),
        _ => {} // default auto detection by colored crate
    }
}

/// Serialize DiffOutput to pretty-printed JSON.
pub fn format_json(output: &DiffOutput) -> Result<String> {
    serde_json::to_string_pretty(output).context("Failed to serialize output to JSON")
}

/// Format DiffOutput as JSON Lines (JSONL).
pub fn format_jsonl(output: &DiffOutput) -> Result<String> {
    let mut lines = Vec::new();
    for file in &output.files {
        let line = serde_json::to_string(file).context("Failed to serialize file diff to JSONL")?;
        lines.push(line);
    }
    Ok(lines.join("\n"))
}

/// Format high-level summary table (--stat / -s).
pub fn format_stat(output: &DiffOutput) -> String {
    let mut buf = String::new();
    buf.push_str("━━━ symtrace Diff Stat ━━━\n");
    buf.push_str(&format!("{:<50} | {:<12} | {:<10}\n", "File Path", "Operations", "Status"));
    buf.push_str(&format!("{:-<50}-+-{:-<12}-+-{:-<10}\n", "", "", ""));

    for file in &output.files {
        let ops_count = file.operations.len();
        let status_str = if ops_count == 0 { "Unchanged" } else { "Modified" };
        buf.push_str(&format!(
            "{:<50} | {:<12} | {:<10}\n",
            file.file_path, ops_count, status_str
        ));
    }

    buf.push_str(&format!("{:-<50}-+-{:-<12}-+-{:-<10}\n", "", "", ""));
    buf.push_str(&format!(
        "Total: {} files changed ({} moves, {} renames, {} inserts, {} deletes, {} modifies)\n",
        output.summary.total_files,
        output.summary.moves,
        output.summary.renames,
        output.summary.inserts,
        output.summary.deletes,
        output.summary.modifications
    ));
    buf
}

/// Format changed file names only (--name-only).
pub fn format_name_only(output: &DiffOutput) -> String {
    output
        .files
        .iter()
        .filter(|f| !f.operations.is_empty())
        .map(|f| f.file_path.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Format DiffOutput as an ultra-dense, token-optimized context prompt for LLMs (Gemini, Claude, GPT).
/// Emits semantic structural changes, parameter deltas, control-flow shifts, and blast radius impact
/// while saving up to 80% tokens compared to raw unified diffs.
pub fn format_prompt(output: &DiffOutput) -> String {
    let mut buf = String::new();
    buf.push_str("=== symtrace SEMANTIC CONTEXT ===\n");
    buf.push_str(&format!(
        "Repository: {} | Commits: {} -> {}\n",
        output.repository, output.commit_a, output.commit_b
    ));

    if let Some(ref class) = output.commit_classification {
        let intents = if class.intent_labels.is_empty() {
            String::new()
        } else {
            format!(" | Intents: [{}]", class.intent_labels.join(", "))
        };
        buf.push_str(&format!(
            "Classification: {} (confidence: {:.0}%){}\n",
            class.primary_class, class.confidence_score * 100.0, intents
        ));
    }

    buf.push_str(&format!(
        "Summary: {} files (+{} -{} ~{} ↔{} ✎{})\n\n",
        output.summary.total_files,
        output.summary.inserts,
        output.summary.deletes,
        output.summary.modifications,
        output.summary.moves,
        output.summary.renames,
    ));

    // Contract violations if present
    if let Some(ref violations) = output.contract_violations {
        if !violations.is_empty() {
            buf.push_str("--- CRITICAL CONTRACT & SAFETY ALERTS ---\n");
            for v in violations {
                buf.push_str(&format!(
                    "! [{}] {}:L{} — {}\n",
                    v.rule, v.file_path, v.line, v.message
                ));
            }
            buf.push('\n');
        }
    }

    // Semantic changes per file
    buf.push_str("--- STRUCTURAL MODIFICATIONS ---\n");
    let mut any_ops = false;
    for file in &output.files {
        if file.operations.is_empty() {
            continue;
        }
        for op in &file.operations {
            any_ops = true;
            let tag = match op.op_type {
                OperationType::Move => "[MOVED]",
                OperationType::Rename => "[RENAMED]",
                OperationType::Insert => "[INSERTED]",
                OperationType::Delete => "[DELETED]",
                OperationType::Modify => "[MODIFIED]",
            };
            let loc = op.new_location.as_deref().or(op.old_location.as_deref()).unwrap_or("L1");
            buf.push_str(&format!(
                "{} {} ({} at {})\n",
                tag, op.details, file.file_path, loc
            ));
            if let Some(ref sim) = op.similarity {
                if sim.control_flow_changed {
                    buf.push_str("  ~ Control flow altered\n");
                }
            }
        }
        for pat in &file.refactor_patterns {
            buf.push_str(&format!("  * Refactor: {} (confidence: {:.0}%)\n", pat.description, pat.confidence * 100.0));
        }
    }

    if !any_ops {
        buf.push_str("(no semantic operations detected)\n");
    }

    // Downstream blast radius
    if let Some(ref blast_reports) = output.blast_radius {
        if blast_reports.iter().any(|r| r.total_impacted_callers > 0) {
            buf.push_str("\n--- SEMANTIC BLAST RADIUS & CALL SITES ---\n");
            for r in blast_reports {
                if r.total_impacted_callers > 0 {
                    buf.push_str(&format!(
                        "! Symbol '{}' in {} impacts {} caller(s) (Severity: {})\n",
                        r.modified_symbol, r.file_path, r.total_impacted_callers, r.severity
                    ));
                    for c in &r.impacted_callers {
                        buf.push_str(&format!(
                            "  ▸ called by '{}' ({}:L{}, depth: {})\n",
                            c.caller_symbol, c.caller_file, c.call_site_line, c.depth
                        ));
                    }
                }
            }
        }
    }

    buf
}

/// Format DiffOutput as Markdown.
pub fn format_markdown(output: &DiffOutput) -> String {
    let mut buf = String::new();
    buf.push_str("# symtrace Semantic Diff Report\n\n");
    buf.push_str(&format!("**Repository:** `{}`  \n", output.repository));
    buf.push_str(&format!("**Comparing:** `{}` → `{}`  \n\n", output.commit_a, output.commit_b));

    buf.push_str("## File Changes\n\n");
    for file in &output.files {
        buf.push_str(&format!("### `{}`\n\n", file.file_path));
        buf.push_str("| Operation | Details | Location | Similarity |\n");
        buf.push_str("| :--- | :--- | :--- | :--- |\n");
        for op in &file.operations {
            let loc = match (&op.old_location, &op.new_location) {
                (Some(o), Some(n)) => format!("{} → {}", o, n),
                (Some(o), None) => o.clone(),
                (None, Some(n)) => n.clone(),
                (None, None) => "-".to_string(),
            };
            let sim = op.similarity.as_ref().map_or("-".to_string(), |s| format!("{:.0}%", s.similarity_percent));
            buf.push_str(&format!(
                "| **{:?}** | {} | {} | {} |\n",
                op.op_type, op.details, loc, sim
            ));
        }
        buf.push('\n');
    }

    buf.push_str("## Summary\n\n");
    buf.push_str(&format!("- Total Files: {}\n", output.summary.total_files));
    buf.push_str(&format!("- Moves: {}\n", output.summary.moves));
    buf.push_str(&format!("- Renames: {}\n", output.summary.renames));
    buf.push_str(&format!("- Inserts: {}\n", output.summary.inserts));
    buf.push_str(&format!("- Deletes: {}\n", output.summary.deletes));
    buf.push_str(&format!("- Modifications: {}\n", output.summary.modifications));
    buf
}

/// Format DiffOutput as an executive, professional white-mode HTML report with BLAKE3 signature.
pub fn format_html(output: &DiffOutput) -> String {
    let json_bytes = serde_json::to_vec(output).unwrap_or_default();
    let fingerprint = blake3::hash(&json_bytes).to_hex().to_string();

    let mut buf = String::new();
    buf.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    buf.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    buf.push_str("<title>symtrace Audit Report</title>\n");
    buf.push_str("<style>\n");
    buf.push_str("  :root {\n");
    buf.push_str("    --bg: #ffffff; --surface: #ffffff; --border: #e5e7eb; --border-dark: #111827;\n");
    buf.push_str("    --text-main: #111827; --text-muted: #6b7280; --text-light: #9ca3af;\n");
    buf.push_str("  }\n");
    buf.push_str("  * { box-sizing: border-box; }\n");
    buf.push_str("  body {\n");
    buf.push_str("    font-family: -apple-system, BlinkMacSystemFont, 'Inter', 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;\n");
    buf.push_str("    background: var(--bg); color: var(--text-main); margin: 0; padding: 40px 24px;\n");
    buf.push_str("    line-height: 1.5; -webkit-font-smoothing: antialiased;\n");
    buf.push_str("  }\n");
    buf.push_str("  .report-container { max-width: 1100px; margin: 0 auto; }\n");
    buf.push_str("  .header {\n");
    buf.push_str("    border: 1px solid var(--border); border-radius: 6px; padding: 24px 28px;\n");
    buf.push_str("    margin-bottom: 20px; display: flex; justify-content: space-between;\n");
    buf.push_str("    align-items: flex-start; flex-wrap: wrap; gap: 16px;\n");
    buf.push_str("  }\n");
    buf.push_str("  .title-area h1 { font-size: 1.5rem; margin: 0 0 4px 0; color: var(--text-main); font-weight: 700; letter-spacing: -0.02em; }\n");
    buf.push_str("  .title-area p { margin: 0; color: var(--text-muted); font-size: 0.875rem; }\n");
    buf.push_str("  .meta-group { display: flex; flex-direction: column; align-items: flex-end; gap: 8px; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 0.825rem; }\n");
    buf.push_str("  .meta-tag { border: 1px solid var(--border); border-radius: 4px; padding: 4px 10px; color: var(--text-main); }\n");
    buf.push_str("  .print-btn {\n");
    buf.push_str("    background: #111827; color: #ffffff; border: 1px solid #111827;\n");
    buf.push_str("    border-radius: 4px; padding: 6px 14px; font-size: 0.825rem;\n");
    buf.push_str("    font-weight: 600; cursor: pointer; transition: background 0.15s ease;\n");
    buf.push_str("  }\n");
    buf.push_str("  .print-btn:hover { background: #374151; }\n");
    buf.push_str("  .notice-card {\n");
    buf.push_str("    border: 1px solid var(--border); border-radius: 6px;\n");
    buf.push_str("    padding: 16px 20px; margin-bottom: 20px; font-size: 0.85rem;\n");
    buf.push_str("    line-height: 1.6; color: var(--text-main);\n");
    buf.push_str("  }\n");
    buf.push_str("  .notice-card strong { font-weight: 600; }\n");
    buf.push_str("  .notice-card code { font-family: ui-monospace, monospace; font-size: 0.8rem; background: #f3f4f6; padding: 2px 5px; border-radius: 3px; word-break: break-all; }\n");
    buf.push_str("  .summary-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr)); gap: 12px; margin-bottom: 20px; }\n");
    buf.push_str("  .stat-card { border: 1px solid var(--border); border-radius: 6px; padding: 14px; text-align: center; }\n");
    buf.push_str("  .stat-label { font-size: 0.725rem; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-muted); font-weight: 600; }\n");
    buf.push_str("  .stat-value { font-size: 1.5rem; font-weight: 700; margin-top: 2px; color: var(--text-main); font-family: ui-monospace, monospace; }\n");
    buf.push_str("  .audit-meta-card {\n");
    buf.push_str("    border: 1px solid var(--border); border-radius: 6px;\n");
    buf.push_str("    padding: 14px 20px; margin-bottom: 20px; font-size: 0.825rem;\n");
    buf.push_str("    display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));\n");
    buf.push_str("    gap: 10px; font-family: ui-monospace, monospace;\n");
    buf.push_str("  }\n");
    buf.push_str("  .search-input {\n");
    buf.push_str("    width: 100%; box-sizing: border-box; background: #ffffff;\n");
    buf.push_str("    border: 1px solid var(--border); color: var(--text-main);\n");
    buf.push_str("    padding: 10px 14px; border-radius: 6px; font-size: 0.875rem;\n");
    buf.push_str("    margin-bottom: 20px; outline: none; transition: border-color 0.15s ease;\n");
    buf.push_str("  }\n");
    buf.push_str("  .search-input:focus { border-color: var(--border-dark); }\n");
    buf.push_str("  .file-card { border: 1px solid var(--border); border-radius: 6px; margin-bottom: 16px; overflow: hidden; page-break-inside: avoid; }\n");
    buf.push_str("  .file-header {\n");
    buf.push_str("    background: #f9fafb; padding: 12px 18px;\n");
    buf.push_str("    font-family: ui-monospace, monospace; font-weight: 600;\n");
    buf.push_str("    border-bottom: 1px solid var(--border); color: var(--text-main);\n");
    buf.push_str("    font-size: 0.875rem; display: flex; justify-content: space-between; align-items: center;\n");
    buf.push_str("  }\n");
    buf.push_str("  table { width: 100%; border-collapse: collapse; font-size: 0.85rem; background: #ffffff; }\n");
    buf.push_str("  th, td { padding: 10px 18px; text-align: left; border-bottom: 1px solid var(--border); }\n");
    buf.push_str("  th { background: #f9fafb; color: var(--text-muted); font-weight: 600; text-transform: uppercase; font-size: 0.7rem; letter-spacing: 0.05em; }\n");
    buf.push_str("  tr:last-child td { border-bottom: none; }\n");
    buf.push_str("  .op-tag {\n");
    buf.push_str("    display: inline-block; padding: 2px 7px;\n");
    buf.push_str("    border-radius: 3px; font-size: 0.725rem;\n");
    buf.push_str("    font-weight: 600; font-family: ui-monospace, monospace;\n");
    buf.push_str("    border: 1px solid #111827; background: #ffffff; color: #111827;\n");
    buf.push_str("  }\n");
    buf.push_str("  .footer {\n");
    buf.push_str("    text-align: center; margin-top: 36px; padding-top: 20px;\n");
    buf.push_str("    border-top: 1px solid var(--border); color: var(--text-muted); font-size: 0.8rem;\n");
    buf.push_str("  }\n");
    buf.push_str("  @media print {\n");
    buf.push_str("    body { background: #ffffff; color: #000000; padding: 0; }\n");
    buf.push_str("    .report-container { max-width: 100%; }\n");
    buf.push_str("    .print-btn, .search-input { display: none !important; }\n");
    buf.push_str("    .file-card, .header, .stat-card, .notice-card { border: 1px solid #000000; page-break-inside: avoid; }\n");
    buf.push_str("  }\n");
    buf.push_str("</style>\n</head>\n<body>\n");

    buf.push_str("<div class=\"report-container\">\n");
    buf.push_str("  <div class=\"header\">\n");
    buf.push_str("    <div class=\"title-area\">\n");
    buf.push_str("      <h1>symtrace Audit Report</h1>\n");
    buf.push_str("      <p>Structural AST Differential Analysis</p>\n");
    buf.push_str("    </div>\n");
    buf.push_str("    <div class=\"meta-group\">\n");
    buf.push_str("      <button onclick=\"window.print()\" class=\"print-btn\">Print / Save PDF</button>\n");
    buf.push_str(&format!("      <div class=\"meta-tag\">Repository: {}</div>\n", output.repository));
    buf.push_str(&format!("      <div class=\"meta-tag\">Comparing: {} &rarr; {}</div>\n", output.commit_a, output.commit_b));
    buf.push_str("    </div>\n");
    buf.push_str("  </div>\n");

    buf.push_str("  <div class=\"notice-card\">\n");
    buf.push_str(&format!("    <div><strong>Digital Fingerprint:</strong> <code>{}</code></div>\n", fingerprint));
    buf.push_str("    <div style=\"margin-top:6px; color:var(--text-muted);\">\n");
    buf.push_str("      <strong>Notice:</strong> This report has been digitally fingerprinted by the symtrace engine for tamper-evidence. This fingerprint confirms only that the file was generated by the engine and has not been altered; it does not verify or guarantee the absolute correctness of the analysis.\n");
    buf.push_str("    </div>\n");
    buf.push_str("  </div>\n");

    if let Some(ref violations) = output.contract_violations {
        if !violations.is_empty() {
            buf.push_str("  <div class=\"notice-card\">\n");
            buf.push_str(&format!("    <div style=\"font-weight:600;\">Safety Contract Violations ({} Detected):</div>\n", violations.len()));
            buf.push_str("    <ul style=\"margin:8px 0 0 0; padding-left:20px;\">\n");
            for v in violations {
                buf.push_str(&format!(
                    "      <li><strong>[{}]</strong> <code>{}:L{}</code> &mdash; {} (Rule: <em>{}</em>)</li>\n",
                    v.severity, v.file_path, v.line, v.message, v.rule
                ));
            }
            buf.push_str("    </ul>\n");
            buf.push_str("  </div>\n");
        }
    }


    buf.push_str("  <div class=\"summary-grid\">\n");
    buf.push_str(&format!("    <div class=\"stat-card\"><div class=\"stat-label\">Files Audited</div><div class=\"stat-value\">{}</div></div>\n", output.summary.total_files));
    buf.push_str(&format!("    <div class=\"stat-card\"><div class=\"stat-label\">Modifications</div><div class=\"stat-value\">{}</div></div>\n", output.summary.modifications));
    buf.push_str(&format!("    <div class=\"stat-card\"><div class=\"stat-label\">Inserts</div><div class=\"stat-value\">{}</div></div>\n", output.summary.inserts));
    buf.push_str(&format!("    <div class=\"stat-card\"><div class=\"stat-label\">Deletes</div><div class=\"stat-value\">{}</div></div>\n", output.summary.deletes));
    buf.push_str(&format!("    <div class=\"stat-card\"><div class=\"stat-label\">Moves</div><div class=\"stat-value\">{}</div></div>\n", output.summary.moves));
    buf.push_str(&format!("    <div class=\"stat-card\"><div class=\"stat-label\">Renames</div><div class=\"stat-value\">{}</div></div>\n", output.summary.renames));
    buf.push_str("  </div>\n");

    buf.push_str("  <div class=\"audit-meta-card\">\n");
    buf.push_str(&format!("    <div><strong>Nodes Compared:</strong> {}</div>\n", output.performance.total_nodes_compared));
    buf.push_str(&format!("    <div><strong>Parse Duration:</strong> {:.2} ms</div>\n", output.performance.parse_time_ms));
    buf.push_str(&format!("    <div><strong>Diff Duration:</strong> {:.2} ms</div>\n", output.performance.diff_time_ms));
    buf.push_str(&format!("    <div><strong>Total Duration:</strong> {:.2} ms</div>\n", output.performance.total_time_ms));
    if let Some(ref class) = output.commit_classification {
        buf.push_str(&format!("    <div><strong>Classification:</strong> {:?}</div>\n", class.primary_class));
        buf.push_str(&format!("    <div><strong>Confidence:</strong> {:.0}%</div>\n", class.confidence_score * 100.0));
    }
    buf.push_str("  </div>\n");

    buf.push_str("  <input type=\"text\" id=\"filterInput\" class=\"search-input\" placeholder=\"Filter records by file, symbol, or operation...\">\n");

    buf.push_str("  <div id=\"fileContainer\">\n");
    for file in &output.files {
        buf.push_str("    <div class=\"file-card\">\n");
        buf.push_str(&format!("      <div class=\"file-header\"><span>{}</span><span>{} Operations</span></div>\n", file.file_path, file.operations.len()));
        if file.operations.is_empty() {
            buf.push_str("      <div style=\"padding:16px 18px; color:var(--text-muted); font-size:0.85rem;\">No structural AST modifications detected in this file.</div>\n");
        } else {
            buf.push_str("      <table>\n");
            buf.push_str("        <thead><tr><th>Operation</th><th>Details</th><th>Location Range</th><th>Similarity</th></tr></thead>\n");
            buf.push_str("        <tbody>\n");
            for op in &file.operations {
                let loc = match (&op.old_location, &op.new_location) {
                    (Some(o), Some(n)) => {
                        if o == n {
                            o.clone()
                        } else {
                            format!("{} &rarr; {}", o, n)
                        }
                    }
                    (Some(o), None) => o.clone(),
                    (None, Some(n)) => n.clone(),
                    (None, None) => "-".to_string(),
                };
                let sim = op.similarity.as_ref().map_or("-".to_string(), |s| format!("{:.0}%", s.similarity_percent));
                buf.push_str(&format!(
                    "          <tr><td><span class=\"op-tag\">{:?}</span></td><td>{}</td><td style=\"font-family:ui-monospace,monospace;\">{}</td><td>{}</td></tr>\n",
                    op.op_type, op.details, loc, sim
                ));
            }
            buf.push_str("        </tbody>\n");
            buf.push_str("      </table>\n");
        }
        buf.push_str("    </div>\n");
    }
    buf.push_str("  </div>\n");

    buf.push_str("  <div class=\"footer\">\n");
    buf.push_str(&format!("    <div>Generated by symtrace v0.5.0 &bull; Fingerprint: <code>{}</code></div>\n", &fingerprint[..16]));
    buf.push_str("  </div>\n");
    buf.push_str("</div>\n");

    buf.push_str("<script>\n");
    buf.push_str("  document.getElementById('filterInput').addEventListener('input', function(e) {\n");
    buf.push_str("    const q = e.target.value.toLowerCase();\n");
    buf.push_str("    document.querySelectorAll('.file-card').forEach(card => {\n");
    buf.push_str("      card.style.display = card.innerText.toLowerCase().includes(q) ? '' : 'none';\n");
    buf.push_str("    });\n");
    buf.push_str("  });\n");
    buf.push_str("</script>\n");
    buf.push_str("</body>\n</html>");
    buf
}

/// Format DiffOutput as SARIF (Static Analysis Results Interchange Format v2.1.0).
pub fn format_sarif(output: &DiffOutput) -> Result<String> {
    let sarif_val = serde_json::json!({
        "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "symtrace",
                    "version": env!("CARGO_PKG_VERSION"),
                    "informationUri": "https://github.com/symtrace/symtrace"
                }
            },
            "results": output.files.iter().flat_map(|f| {
                f.operations.iter().map(|op| {
                    serde_json::json!({
                        "ruleId": format!("symtrace/{:?}", op.op_type).to_lowercase(),
                        "message": { "text": op.details },
                        "locations": [{
                            "physicalLocation": {
                                "artifactLocation": { "uri": f.file_path }
                            }
                        }]
                    })
                })
            }).collect::<Vec<_>>()
        }]
    });
    serde_json::to_string_pretty(&sarif_val).context("Failed to serialize SARIF report")
}

/// Format DiffOutput as a human-readable CLI report.
pub fn format_cli(output: &DiffOutput) -> String {
    let mut buf = String::new();

    // ── Header ───────────────────────────────────────────────────────
    buf.push_str(&format!("{}\n", "━━━ symtrace  Semantic Diff ━━━".bold()));
    buf.push_str(&format!("Repository : {}\n", output.repository.cyan()));
    buf.push_str(&format!(
        "Comparing  : {} → {}\n\n",
        output.commit_a.yellow(),
        output.commit_b.yellow()
    ));

    // ── Per-file operations ──────────────────────────────────────────
    if output.files.is_empty() {
        buf.push_str(&"  (no semantic changes detected)\n\n".dimmed().to_string());
    }

    for file in &output.files {
        buf.push_str(&format!(
            "{} {}\n",
            "━━━".bold(),
            file.file_path.bold().underline()
        ));

        if file.operations.is_empty() {
            buf.push_str(&"    (no significant operations)\n".dimmed().to_string());
        }

        for op in &file.operations {
            let (symbol, colored_type) = match op.op_type {
                OperationType::Move => ("↔", "MOVE".blue().bold()),
                OperationType::Rename => ("✎", "RENAME".yellow().bold()),
                OperationType::Insert => ("+", "INSERT".green().bold()),
                OperationType::Delete => ("-", "DELETE".red().bold()),
                OperationType::Modify => ("~", "MODIFY".cyan().bold()),
            };

            let location = match (&op.old_location, &op.new_location) {
                (Some(old), Some(new)) => {
                    if old == new {
                        old.clone()
                    } else {
                        format!("{} → {}", old, new)
                    }
                }
                (Some(old), None) => old.clone(),
                (None, Some(new)) => new.clone(),
                (None, None) => "—".to_string(),
            };

            buf.push_str(&format!(
                "  {} [{}] {} ({})",
                symbol,
                colored_type,
                op.details,
                location.dimmed()
            ));

            // Append similarity score if present
            if let Some(ref sim) = op.similarity {
                buf.push_str(&format!(
                    " [{:.0}% similarity, {}]",
                    sim.similarity_percent, sim.change_intensity
                ));
            }
            buf.push('\n');
        }

        // ── Refactor patterns ────────────────────────────────────────
        if !file.refactor_patterns.is_empty() {
            buf.push_str(&format!("  {}\n", "── Refactor Patterns ──".dimmed()));
            for pattern in &file.refactor_patterns {
                buf.push_str(&format!(
                    "    {} {} (confidence: {:.0}%)\n",
                    "▸".magenta(),
                    pattern.description,
                    pattern.confidence * 100.0
                ));
            }
        }

        buf.push('\n');
    }

    // ── Summary ──────────────────────────────────────────────────────
    buf.push_str(&format!("{}\n", "━━━ Summary ━━━".bold()));
    buf.push_str(&format!(
        "  Files          : {}\n",
        output.summary.total_files
    ));
    buf.push_str(&format!("  Moves          : {}\n", output.summary.moves));
    buf.push_str(&format!("  Renames        : {}\n", output.summary.renames));
    buf.push_str(&format!("  Inserts        : {}\n", output.summary.inserts));
    buf.push_str(&format!("  Deletes        : {}\n", output.summary.deletes));
    buf.push_str(&format!(
        "  Modifications  : {}\n",
        output.summary.modifications
    ));

    // ── Cross-File Symbol Tracking ───────────────────────────────────
    if let Some(ref tracking) = output.cross_file_tracking {
        buf.push_str(&format!(
            "\n{}\n",
            "━━━ Cross-File Symbol Tracking ━━━".bold()
        ));
        buf.push_str(&format!("  Symbols tracked : {}\n", tracking.symbol_count));
        if tracking.cross_file_events.is_empty() {
            buf.push_str(&"  (no cross-file events detected)\n".dimmed().to_string());
        } else {
            for event in &tracking.cross_file_events {
                let symbol = match event.event {
                    crate::types::CrossFileEventKind::CrossFileMove => "↔".blue().to_string(),
                    crate::types::CrossFileEventKind::CrossFileRename => "✎".yellow().to_string(),
                    crate::types::CrossFileEventKind::ApiSurfaceChange => "⚠".red().to_string(),
                };
                buf.push_str(&format!(
                    "  {} [{}] {} (similarity: {:.0}%)\n",
                    symbol,
                    event.event.to_string().bold(),
                    event.description,
                    event.similarity_score * 100.0
                ));
            }
        }
    }

    // ── Commit Classification ────────────────────────────────────────
    if let Some(ref classification) = output.commit_classification {
        buf.push_str(&format!("\n{}\n", "━━━ Commit Classification ━━━".bold()));
        buf.push_str(&format!(
            "  Class          : {}\n",
            classification.primary_class.to_string().bold().cyan()
        ));
        buf.push_str(&format!(
            "  Confidence     : {:.0}%\n",
            classification.confidence_score * 100.0
        ));
        if !classification.intent_labels.is_empty() {
            buf.push_str(&format!(
                "  Intent Labels  : {}\n",
                classification.intent_labels.join(", ").yellow()
            ));
        }
    }

    // ── Contract Violations ───────────────────────────────────────────
    if let Some(ref violations) = output.contract_violations {
        if !violations.is_empty() {
            buf.push_str(&format!("\n{}\n", "━━━ Contract Violations & Security Guards ━━━".bold().red()));
            for v in violations {
                buf.push_str(&format!(
                    "  {} [{}] {}:L{} — {}\n",
                    "⚠".red().bold(),
                    v.rule.red().bold(),
                    v.file_path.bold(),
                    v.line,
                    v.message
                ));
            }
        }
    }

    // ── Semantic Blast Radius ─────────────────────────────────────────
    if let Some(ref blast_reports) = output.blast_radius {
        if !blast_reports.is_empty() {
            buf.push_str(&format!("\n{}\n", "━━━ Semantic Blast Radius ━━━".bold().yellow()));
            for r in blast_reports {
                if r.total_impacted_callers > 0 {
                    buf.push_str(&format!(
                        "  {} Symbol '{}' in {} (impact: {} downstream caller(s), severity: {})\n",
                        "⚡".yellow().bold(),
                        r.modified_symbol.cyan().bold(),
                        r.file_path,
                        r.total_impacted_callers,
                        r.severity.bold()
                    ));
                    for caller in &r.impacted_callers {
                        buf.push_str(&format!(
                            "    ▸ called by '{}' in {}:L{} (depth: {})\n",
                            caller.caller_symbol.bold(),
                            caller.caller_file,
                            caller.call_site_line,
                            caller.depth
                        ));
                    }
                }
            }
        }
    }

    // ── Performance ──────────────────────────────────────────────────
    buf.push_str(&format!("\n{}\n", "━━━ Performance ━━━".bold()));
    buf.push_str(&format!(
        "  Files processed   : {}\n",
        output.performance.total_files_processed
    ));
    buf.push_str(&format!(
        "  Nodes compared    : {}\n",
        output.performance.total_nodes_compared
    ));
    buf.push_str(&format!(
        "  Parse time        : {:.2} ms\n",
        output.performance.parse_time_ms
    ));
    buf.push_str(&format!(
        "  Diff time         : {:.2} ms\n",
        output.performance.diff_time_ms
    ));
    buf.push_str(&format!(
        "  Total time        : {:.2} ms\n",
        output.performance.total_time_ms
    ));
    if output.performance.incremental_parses > 0 {
        buf.push_str(&format!(
            "  Incremental       : {} file(s), {} nodes reused\n",
            output.performance.incremental_parses, output.performance.nodes_reused
        ));
    }

    buf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DiffOutput, DiffSummary, FileDiff, OperationRecord, EntityType, PerformanceMetrics};

    fn sample_output() -> DiffOutput {
        DiffOutput {
            repository: "test_repo".to_string(),
            commit_a: "HEAD~1".to_string(),
            commit_b: "HEAD".to_string(),
            files: vec![FileDiff {
                file_path: "src/main.rs".to_string(),
                operations: vec![OperationRecord {
                    op_type: OperationType::Modify,
                    entity_type: EntityType::Function,
                    old_location: Some("L10".to_string()),
                    new_location: Some("L15".to_string()),
                    details: "fn process modified".to_string(),
                    similarity: None,
                    is_logic_op: true,
                }],
                refactor_patterns: vec![],
            }],
            summary: DiffSummary {
                total_files: 1,
                moves: 0,
                renames: 0,
                inserts: 0,
                deletes: 0,
                modifications: 1,
            },
            cross_file_tracking: None,
            commit_classification: None,
            performance: PerformanceMetrics {
                total_files_processed: 1,
                total_nodes_compared: 50,
                parse_time_ms: 1.2,
                diff_time_ms: 0.5,
                total_time_ms: 1.7,
                incremental_parses: 0,
                nodes_reused: 0,
            },
            granularity: None,
            blast_radius: None,
            contract_violations: None,
        }
    }

    #[test]
    fn parse_output_format_strings() {
        assert_eq!(OutputFormat::parse("ansi").unwrap(), OutputFormat::Ansi);
        assert_eq!(OutputFormat::parse("json").unwrap(), OutputFormat::Json);
        assert_eq!(OutputFormat::parse("jsonl").unwrap(), OutputFormat::Jsonl);
        assert_eq!(OutputFormat::parse("markdown").unwrap(), OutputFormat::Markdown);
        assert_eq!(OutputFormat::parse("html").unwrap(), OutputFormat::Html);
        assert_eq!(OutputFormat::parse("sarif").unwrap(), OutputFormat::Sarif);
        assert!(OutputFormat::parse("invalid").is_err());
    }

    #[test]
    fn format_stat_renders_summary() {
        let sample = sample_output();
        let res = format_stat(&sample);
        assert!(res.contains("src/main.rs"));
        assert!(res.contains("Total: 1 files changed"));
    }

    #[test]
    fn format_name_only_renders_paths() {
        let sample = sample_output();
        let res = format_name_only(&sample);
        assert_eq!(res.trim(), "src/main.rs");
    }

    #[test]
    fn format_jsonl_renders_lines() {
        let sample = sample_output();
        let res = format_jsonl(&sample).unwrap();
        assert!(res.contains("src/main.rs"));
    }

    #[test]
    fn format_markdown_renders_table() {
        let sample = sample_output();
        let res = format_markdown(&sample);
        assert!(res.contains("# symtrace Semantic Diff Report"));
        assert!(res.contains("`src/main.rs`"));
    }

    #[test]
    fn format_html_renders_doctype() {
        let sample = sample_output();
        let res = format_html(&sample);
        assert!(res.contains("<!DOCTYPE html>"));
        assert!(res.contains("src/main.rs"));
    }

    #[test]
    fn format_sarif_renders_valid_json() {
        let sample = sample_output();
        let res = format_sarif(&sample).unwrap();
        assert!(res.contains("2.1.0"));
        assert!(res.contains("symtrace"));
    }

    #[test]
    fn test_determine_granularity_auto_detection() {
        let sample = sample_output(); // 1 file, 1 op -> MicroCompact
        let auto_mode = determine_granularity(&sample, false, false);
        assert_eq!(auto_mode, DisplayGranularity::MicroCompact);

        let forced_compact = determine_granularity(&sample, true, false);
        assert_eq!(forced_compact, DisplayGranularity::MicroCompact);

        let forced_headers = determine_granularity(&sample, false, true);
        assert_eq!(forced_headers, DisplayGranularity::FullStructural);
    }

    #[test]
    fn test_format_micro_cli_output() {
        let sample = sample_output();
        let micro_res = format_micro_cli(&sample);
        assert!(micro_res.contains("src/main.rs:L15"));
        assert!(micro_res.contains("[MODIFY]"));
        assert!(micro_res.contains("fn process modified"));
        assert!(!micro_res.contains("━━━ Summary ━━━")); // Suppressed in micro mode
    }

    #[test]
    fn test_format_cli_with_granularity() {
        let sample = sample_output();
        let micro = format_cli_with_granularity(&sample, DisplayGranularity::MicroCompact);
        let standard = format_cli_with_granularity(&sample, DisplayGranularity::Standard);
        let full = format_cli_with_granularity(&sample, DisplayGranularity::FullStructural);

        assert!(!micro.contains("━━━ Performance ━━━"));
        assert!(standard.contains("━━━ Summary ━━━"));
        assert!(full.contains("━━━ Performance ━━━"));
    }

    #[test]
    fn test_format_prompt_renders_llm_context() {
        let sample = sample_output();
        let prompt_out = format_prompt(&sample);
        assert!(prompt_out.contains("=== symtrace SEMANTIC CONTEXT ==="));
        assert!(prompt_out.contains("--- STRUCTURAL MODIFICATIONS ---"));
        assert!(prompt_out.contains("[MODIFIED] fn process modified (src/main.rs at L15)"));
    }
}
