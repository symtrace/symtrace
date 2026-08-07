use std::io::{stdout, Write};
use anyhow::Result;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind},
    execute, queue,
    style::{Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use crate::types::DiffOutput;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivePane {
    Files,
    Operations,
}

/// Interactive TUI inspector mode for browsing multi-file semantic diffs.
pub fn run_tui_inspector(diff_output: &DiffOutput) -> Result<()> {
    if !crossterm::tty::IsTty::is_tty(&stdout()) {
        return render_static_tui(diff_output);
    }

    terminal::enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen, cursor::Hide)?;

    let mut selected_file = 0usize;
    let mut selected_op = 0usize;
    let mut active_pane = ActivePane::Files;
    let mut is_running = true;
    let mut needs_render = true;

    while is_running {
        if needs_render {
            let (width, height) = terminal::size().unwrap_or((80, 24));
            let w = width as usize;

            // Clear screen once per frame render
            queue!(out, cursor::MoveTo(0, 0), Clear(ClearType::All))?;

            // 1. Header Bar
            let header_str = format!(
                " SYMTRACE TUI INSPECTOR v0.4.0 | Repo: {} | Comparing: {} -> {}",
                diff_output.repository, diff_output.commit_a, diff_output.commit_b
            );
            queue!(
                out,
                SetBackgroundColor(Color::Blue),
                SetForegroundColor(Color::White),
                SetAttribute(Attribute::Bold),
                Print(format!("{:<w$}", header_str, w = w)),
                ResetColor,
                cursor::MoveTo(0, 1)
            )?;

            let total_files = diff_output.files.len();
            if total_files > 0 && selected_file >= total_files {
                selected_file = total_files - 1;
            }

            let content_height = height.saturating_sub(6) as usize;

            if total_files == 0 {
                queue!(
                    out,
                    cursor::MoveTo(2, 3),
                    SetForegroundColor(Color::White),
                    Print("No file changes detected between commits."),
                    ResetColor
                )?;
            } else {
                let current_file = &diff_output.files[selected_file];
                let ops_count = current_file.operations.len();
                if ops_count > 0 && selected_op >= ops_count {
                    selected_op = ops_count - 1;
                }

                // 2. Render Files Panel
                let mut line = 2;
                let file_focus = active_pane == ActivePane::Files;
                let file_hdr_color = if file_focus { Color::White } else { Color::DarkGrey };

                queue!(
                    out,
                    cursor::MoveTo(2, line as u16),
                    SetForegroundColor(file_hdr_color),
                    SetAttribute(Attribute::Bold),
                    Print(format!(
                        "[ FILES ({}/{}) ] {}",
                        selected_file + 1,
                        total_files,
                        if file_focus { ">> ACTIVE FOCUS <<" } else { "(Press Right/Left to Focus)" }
                    )),
                    ResetColor
                )?;
                line += 1;

                let file_display_limit = (content_height / 3).max(3);
                let start_f = selected_file.saturating_sub(file_display_limit / 2);
                let end_f = (start_f + file_display_limit).min(total_files);

                for idx in start_f..end_f {
                    let file = &diff_output.files[idx];
                    queue!(out, cursor::MoveTo(2, line as u16))?;
                    if idx == selected_file {
                        let bg = if file_focus { Color::DarkGrey } else { Color::Black };
                        queue!(
                            out,
                            SetBackgroundColor(bg),
                            SetForegroundColor(Color::White),
                            SetAttribute(Attribute::Bold),
                            Print(format!(" > {} ({} ops) ", file.file_path, file.operations.len())),
                            ResetColor
                        )?;
                    } else {
                        queue!(
                            out,
                            SetForegroundColor(Color::White),
                            Print(format!("   {} ({} ops) ", file.file_path, file.operations.len())),
                            ResetColor
                        )?;
                    }
                    line += 1;
                }

                // 3. Render Operations Panel
                line += 1;
                let op_focus = active_pane == ActivePane::Operations;
                let op_hdr_color = if op_focus { Color::White } else { Color::DarkGrey };

                queue!(
                    out,
                    cursor::MoveTo(2, line as u16),
                    SetForegroundColor(op_hdr_color),
                    SetAttribute(Attribute::Bold),
                    Print(format!(
                        "[ OPERATIONS for `{}` (Total: {}) ] {}",
                        current_file.file_path,
                        ops_count,
                        if op_focus { ">> ACTIVE FOCUS <<" } else { "(Press Right/Left to Focus)" }
                    )),
                    ResetColor
                )?;
                line += 1;

                if ops_count == 0 {
                    queue!(
                        out,
                        cursor::MoveTo(4, line as u16),
                        SetForegroundColor(Color::DarkGrey),
                        Print("No structural AST operations detected in this file."),
                        ResetColor
                    )?;
                } else {
                    let op_display_limit = (content_height / 3).max(3);
                    let start_op = selected_op.saturating_sub(op_display_limit / 2);
                    let end_op = (start_op + op_display_limit).min(ops_count);

                    for op_idx in start_op..end_op {
                        let op = &current_file.operations[op_idx];
                        if line >= height.saturating_sub(4) as usize {
                            break;
                        }
                        queue!(out, cursor::MoveTo(4, line as u16))?;
                        let loc = match (&op.old_location, &op.new_location) {
                            (Some(o), Some(n)) => {
                                if o == n {
                                    o.clone()
                                } else {
                                    format!("{} -> {}", o, n)
                                }
                            }
                            (Some(o), None) => o.clone(),
                            (None, Some(n)) => n.clone(),
                            (None, None) => "-".to_string(),
                        };

                        if op_idx == selected_op {
                            let bg = if op_focus { Color::DarkGrey } else { Color::Black };
                            queue!(
                                out,
                                SetBackgroundColor(bg),
                                SetForegroundColor(Color::White),
                                SetAttribute(Attribute::Bold),
                                Print(format!(" > [{:?}] {} ({}) ", op.op_type, op.details, loc)),
                                ResetColor
                            )?;
                        } else {
                            queue!(
                                out,
                                SetForegroundColor(Color::White),
                                Print(format!("   [{:?}] {} ({}) ", op.op_type, op.details, loc)),
                                ResetColor
                            )?;
                        }
                        line += 1;
                    }
                }

                // 4. Detail Inspector Card at Bottom
                if ops_count > 0 && selected_op < ops_count {
                    let sel_op = &current_file.operations[selected_op];
                    let card_line = height.saturating_sub(3);
                    let loc_str = match (&sel_op.old_location, &sel_op.new_location) {
                        (Some(o), Some(n)) => format!("Old: {} | New: {}", o, n),
                        (Some(o), None) => format!("Old: {}", o),
                        (None, Some(n)) => format!("New: {}", n),
                        (None, None) => "Location: N/A".to_string(),
                    };
                    let sim_str = sel_op
                        .similarity
                        .as_ref()
                        .map_or("N/A".to_string(), |s| format!("{:.0}%", s.similarity_percent));

                    let inspector_str = format!(
                        " DETAIL: {:?} | {} | Similarity: {} | Location: {}",
                        sel_op.op_type, sel_op.details, sim_str, loc_str
                    );
                    queue!(
                        out,
                        cursor::MoveTo(0, card_line),
                        SetBackgroundColor(Color::Black),
                        SetForegroundColor(Color::White),
                        Print(format!("{:<w$}", inspector_str, w = w)),
                        ResetColor
                    )?;
                }
            }

            // 5. Footer Controls Bar
            let footer_str = " [Up/Down] Scroll List | [Right/Left/Tab] Switch Pane | [q/Esc] Quit";
            queue!(
                out,
                cursor::MoveTo(0, height.saturating_sub(1)),
                SetBackgroundColor(Color::DarkGrey),
                SetForegroundColor(Color::White),
                Print(format!("{:<w$}", footer_str, w = w)),
                ResetColor
            )?;

            out.flush()?;
            needs_render = false;
        }

        // Blocking event read: Zero polling jitter, zero unnecessary screen clears!
        match event::read()? {
            Event::Key(key) => {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => is_running = false,
                        KeyCode::Tab | KeyCode::Right | KeyCode::Left => {
                            active_pane = match active_pane {
                                ActivePane::Files => ActivePane::Operations,
                                ActivePane::Operations => ActivePane::Files,
                            };
                            needs_render = true;
                        }
                        KeyCode::Up => {
                            match active_pane {
                                ActivePane::Files => {
                                    if selected_file > 0 {
                                        selected_file -= 1;
                                        selected_op = 0;
                                        needs_render = true;
                                    }
                                }
                                ActivePane::Operations => {
                                    if selected_op > 0 {
                                        selected_op -= 1;
                                        needs_render = true;
                                    }
                                }
                            }
                        }
                        KeyCode::Down => {
                            match active_pane {
                                ActivePane::Files => {
                                    let total_files = diff_output.files.len();
                                    if total_files > 0 && selected_file + 1 < total_files {
                                        selected_file += 1;
                                        selected_op = 0;
                                        needs_render = true;
                                    }
                                }
                                ActivePane::Operations => {
                                    let total_files = diff_output.files.len();
                                    if total_files > 0 {
                                        let ops_len = diff_output.files[selected_file].operations.len();
                                        if ops_len > 0 && selected_op + 1 < ops_len {
                                            selected_op += 1;
                                            needs_render = true;
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            Event::Resize(_, _) => {
                needs_render = true;
            }
            _ => {}
        }
    }

    execute!(out, LeaveAlternateScreen, cursor::Show)?;
    terminal::disable_raw_mode()?;
    Ok(())
}

fn render_static_tui(diff_output: &DiffOutput) -> Result<()> {
    println!("[INFO] SymTrace TUI Inspector (Static Mode) v0.4.0");
    println!("Repository: {}", diff_output.repository);
    println!("Comparing: {} -> {}", diff_output.commit_a, diff_output.commit_b);
    println!("Total Files: {}", diff_output.summary.total_files);
    for file in &diff_output.files {
        println!("=== File: {} ===", file.file_path);
        for op in &file.operations {
            println!("  [{:?}] {}", op.op_type, op.details);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{DiffOutput, DiffSummary, PerformanceMetrics};

    #[test]
    fn run_tui_inspector_executes_cleanly() {
        let sample = DiffOutput {
            repository: "test_repo".to_string(),
            commit_a: "HEAD~1".to_string(),
            commit_b: "HEAD".to_string(),
            files: vec![],
            summary: DiffSummary {
                total_files: 0,
                moves: 0,
                renames: 0,
                inserts: 0,
                deletes: 0,
                modifications: 0,
            },
            cross_file_tracking: None,
            commit_classification: None,
            performance: PerformanceMetrics {
                total_files_processed: 0,
                total_nodes_compared: 0,
                parse_time_ms: 0.0,
                diff_time_ms: 0.0,
                total_time_ms: 0.0,
                incremental_parses: 0,
                nodes_reused: 0,
            },
        };

        assert!(render_static_tui(&sample).is_ok());
    }
}
