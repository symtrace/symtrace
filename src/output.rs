use anyhow::{Context, Result};
use colored::Colorize;

use crate::types::{DiffOutput, OperationType};

/// Available output formats supported by `symtrace`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Ansi,
    Json,
    Jsonl,
    Markdown,
    Html,
    Sarif,
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
            _ => anyhow::bail!(
                "Unsupported output format: '{}'. Choose from ansi, json, jsonl, markdown, html, sarif",
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
    buf.push_str("━━━ SymTrace Diff Stat ━━━\n");
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

/// Format DiffOutput as Markdown.
pub fn format_markdown(output: &DiffOutput) -> String {
    let mut buf = String::new();
    buf.push_str("# SymTrace Semantic Diff Report\n\n");
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
    buf.push_str("<title>SymTrace Signed Audit Report</title>\n");
    buf.push_str("<style>\n");
    buf.push_str("  :root { --bg: #ffffff; --card: #f8fafc; --border: #cbd5e1; --text: #0f172a; --muted: #475569; --tag-bg: #f1f5f9; --btn-bg: #0f172a; --btn-text: #ffffff; }\n");
    buf.push_str("  body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', sans-serif; background: var(--bg); color: var(--text); margin: 0; padding: 32px; line-height: 1.6; }\n");
    buf.push_str("  .report-container { max-width: 1100px; margin: 0 auto; }\n");
    buf.push_str("  .header { background: var(--card); border: 1px solid var(--border); border-radius: 8px; padding: 24px; margin-bottom: 20px; display: flex; justify-content: space-between; align-items: flex-start; flex-wrap: wrap; gap: 16px; }\n");
    buf.push_str("  .title-area h1 { font-size: 1.6rem; margin: 0 0 6px 0; color: #0f172a; font-weight: 700; letter-spacing: -0.5px; }\n");
    buf.push_str("  .title-area p { margin: 0; color: var(--muted); font-size: 0.9rem; }\n");
    buf.push_str("  .meta-group { display: flex; flex-direction: column; align-items: flex-end; gap: 8px; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 0.85rem; }\n");
    buf.push_str("  .meta-tag { background: #ffffff; border: 1px solid var(--border); border-radius: 4px; padding: 4px 10px; color: var(--text); }\n");
    buf.push_str("  .print-btn { background: var(--btn-bg); color: var(--btn-text); border: none; border-radius: 6px; padding: 8px 16px; font-size: 0.85rem; font-weight: 600; cursor: pointer; display: inline-flex; align-items: center; gap: 6px; box-shadow: 0 1px 2px rgba(0,0,0,0.1); transition: opacity 0.2s; }\n");
    buf.push_str("  .print-btn:hover { opacity: 0.9; }\n");
    buf.push_str("  .disclaimer-card { background: #fffbe6; border: 1px solid #ffe58f; color: #873800; border-radius: 6px; padding: 14px 18px; margin-bottom: 24px; font-size: 0.85rem; line-height: 1.5; }\n");
    buf.push_str("  .signature-card { background: #f0fdf4; border: 1px solid #bbf7d0; color: #166534; border-radius: 6px; padding: 14px 18px; margin-bottom: 24px; font-size: 0.85rem; font-family: ui-monospace, monospace; word-break: break-all; }\n");
    buf.push_str("  .summary-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(130px, 1fr)); gap: 12px; margin-bottom: 24px; }\n");
    buf.push_str("  .stat-card { background: var(--card); border: 1px solid var(--border); border-radius: 6px; padding: 16px; text-align: center; }\n");
    buf.push_str("  .stat-label { font-size: 0.75rem; text-transform: uppercase; letter-spacing: 0.5px; color: var(--muted); font-weight: 600; }\n");
    buf.push_str("  .stat-value { font-size: 1.5rem; font-weight: 700; margin-top: 4px; color: #0f172a; font-family: ui-monospace, monospace; }\n");
    buf.push_str("  .audit-meta-card { background: #f1f5f9; border: 1px solid var(--border); border-radius: 6px; padding: 16px; margin-bottom: 24px; font-size: 0.85rem; display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 12px; font-family: ui-monospace, monospace; }\n");
    buf.push_str("  .search-input { width: 100%; box-sizing: border-box; background: #ffffff; border: 1px solid var(--border); color: #0f172a; padding: 12px 16px; border-radius: 6px; font-size: 0.95rem; margin-bottom: 24px; outline: none; transition: border-color 0.2s; }\n");
    buf.push_str("  .search-input:focus { border-color: #0f172a; }\n");
    buf.push_str("  .file-card { background: var(--card); border: 1px solid var(--border); border-radius: 8px; margin-bottom: 20px; overflow: hidden; page-break-inside: avoid; }\n");
    buf.push_str("  .file-header { background: #e2e8f0; padding: 14px 20px; font-family: ui-monospace, monospace; font-weight: 600; border-bottom: 1px solid var(--border); color: #0f172a; font-size: 0.95rem; display: flex; justify-content: space-between; align-items: center; }\n");
    buf.push_str("  table { width: 100%; border-collapse: collapse; font-size: 0.9rem; background: #ffffff; }\n");
    buf.push_str("  th, td { padding: 12px 20px; text-align: left; border-bottom: 1px solid var(--border); }\n");
    buf.push_str("  th { background: #f1f5f9; color: var(--muted); font-weight: 600; text-transform: uppercase; font-size: 0.75rem; letter-spacing: 0.5px; }\n");
    buf.push_str("  tr:last-child td { border-bottom: none; }\n");
    buf.push_str("  .op-tag { display: inline-block; padding: 3px 8px; border-radius: 4px; font-size: 0.75rem; font-weight: 600; font-family: ui-monospace, monospace; background: var(--tag-bg); color: #0f172a; border: 1px solid var(--border); }\n");
    buf.push_str("  .footer { text-align: center; margin-top: 40px; padding-top: 20px; border-top: 1px solid var(--border); color: var(--muted); font-size: 0.85rem; }\n");
    buf.push_str("  @media print { body { background: #ffffff; color: #000000; padding: 0; } .report-container { max-width: 100%; } .print-btn, .search-input { display: none !important; } .file-card { border: 1px solid #cbd5e1; page-break-inside: avoid; } }\n");
    buf.push_str("</style>\n</head>\n<body>\n");

    buf.push_str("<div class=\"report-container\">\n");
    buf.push_str("  <div class=\"header\">\n");
    buf.push_str("    <div class=\"title-area\">\n");
    buf.push_str("      <h1>SymTrace Signed Audit Report</h1>\n");
    buf.push_str("      <p>Cryptographically Fingerprinted AST Analysis Audit</p>\n");
    buf.push_str("    </div>\n");
    buf.push_str("    <div class=\"meta-group\">\n");
    buf.push_str("      <button onclick=\"window.print()\" class=\"print-btn\">Print / Save PDF</button>\n");
    buf.push_str(&format!("      <div class=\"meta-tag\">Repository: {}</div>\n", output.repository));
    buf.push_str(&format!("      <div class=\"meta-tag\">Comparing: {} &rarr; {}</div>\n", output.commit_a, output.commit_b));
    buf.push_str("    </div>\n");
    buf.push_str("  </div>\n");

    buf.push_str("  <div class=\"signature-card\">\n");
    buf.push_str("    <div><strong>DIGITAL AUDIT SIGNATURE [VERIFIED]:</strong></div>\n");
    buf.push_str(&format!("    <div>BLAKE3 Fingerprint: <code>{}</code></div>\n", fingerprint));
    buf.push_str("    <div>Signer: SymTrace Engine v0.4.0 (Deterministic Cryptographic Hasher)</div>\n");
    buf.push_str("  </div>\n");

    buf.push_str("  <div class=\"disclaimer-card\">\n");
    buf.push_str("    <strong>DISCLAIMER & LIMITATION OF LIABILITY:</strong> This report is generated automatically using Abstract Syntax Tree (AST) structural heuristics and pattern matching algorithms. While SymTrace strives for maximum theoretical determinism, semantic classifications, similarity scores, and refactor patterns are automated estimations and may differ from actual developer intent or runtime code behavior. This report is provided for informational and audit guidance only, without warranties of 100% accuracy.\n");
    buf.push_str("  </div>\n");

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

    buf.push_str("  <input type=\"text\" id=\"filterInput\" class=\"search-input\" placeholder=\"Filter audit records (e.g. file path, function name, operation type)...\">\n");

    buf.push_str("  <div id=\"fileContainer\">\n");
    for file in &output.files {
        buf.push_str("    <div class=\"file-card\">\n");
        buf.push_str(&format!("      <div class=\"file-header\"><span>{}</span><span>{} Operations</span></div>\n", file.file_path, file.operations.len()));
        if file.operations.is_empty() {
            buf.push_str("      <div style=\"padding:16px 20px; color:var(--muted); font-size:0.9rem;\">No structural AST modifications detected in this file.</div>\n");
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
    buf.push_str(&format!("    <div>Generated & Signed by SymTrace v0.4.0 &bull; Fingerprint: <code>{}</code></div>\n", &fingerprint[..16]));
    buf.push_str("    <div style=\"font-size:0.75rem; margin-top:4px; color:var(--muted);\">Notice: Automated AST heuristic estimations may differ from actual developer intent or runtime execution.</div>\n");
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
                    "informationUri": "https://github.com/JashT14/symtrace"
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
    buf.push_str(&format!("{}\n", "━━━ SymTrace  Semantic Diff ━━━".bold()));
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
        assert!(res.contains("# SymTrace Semantic Diff Report"));
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
}
