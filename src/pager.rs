use std::io::{self, IsTerminal, Write};
use std::process::{Child, Command, Stdio};

pub struct Pager {
    child: Option<Child>,
}

impl Pager {
    /// Initialize a shell pager if stdout is a TTY and no_pager is false.
    pub fn setup(no_pager: bool) -> Self {
        if no_pager || !io::stdout().is_terminal() {
            return Pager { child: None };
        }

        let pager_cmd = std::env::var("GIT_PAGER")
            .or_else(|_| std::env::var("PAGER"))
            .unwrap_or_else(|_| {
                if cfg!(target_os = "windows") {
                    "more".to_string()
                } else {
                    "less -RFX".to_string()
                }
            });

        if pager_cmd.is_empty() || pager_cmd == "cat" {
            return Pager { child: None };
        }

        let parts: Vec<&str> = pager_cmd.split_whitespace().collect();
        if parts.is_empty() {
            return Pager { child: None };
        }

        let mut cmd = Command::new(parts[0]);
        if parts.len() > 1 {
            cmd.args(&parts[1..]);
        }
        cmd.stdin(Stdio::piped());

        match cmd.spawn() {
            Ok(child) => Pager { child: Some(child) },
            Err(_) => Pager { child: None },
        }
    }

    /// Write content either to the active pager stdin or directly to stdout.
    pub fn print_output(&mut self, content: &str) -> io::Result<()> {
        if let Some(ref mut child) = self.child {
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(content.as_bytes())?;
                return Ok(());
            }
        }
        io::stdout().write_all(content.as_bytes())?;
        io::stdout().flush()
    }

    /// Wait for the pager process to finish before exit.
    pub fn finish(mut self) {
        if let Some(mut child) = self.child.take() {
            drop(child.stdin.take());
            let _ = child.wait();
        }
    }
}
