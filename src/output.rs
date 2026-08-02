use anyhow::{Context, Result};
use colored::Colorize;

use crate::types::{DiffOutput, OperationType};

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
                (Some(old), Some(new)) => format!("{} → {}", old, new),
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
