use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "symtrace")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Deterministic semantic diff engine using AST-based structural analysis")]
#[command(
    long_about = "symtrace is a deterministic semantic diff engine written in Rust that compares Git \
    commits, staged index changes, or working tree modifications using AST-based structural analysis \
    instead of line-based text diffs.\n\n\
    SUPPORTED LANGUAGES (13):\n  \
      • Rust (.rs)\n  \
      • JavaScript (.js, .jsx, .mjs, .cjs)\n  \
      • TypeScript (.ts, .tsx)\n  \
      • Python (.py, .pyi)\n  \
      • Java (.java)\n  \
      • C (.c, .h)\n  \
      • C++ (.cpp, .hpp, .cc, .cxx, .h++)\n  \
      • Go (.go)\n  \
      • JSON (.json, .jsonc)\n  \
      • C# (.cs)\n  \
      • Ruby (.rb)\n  \
      • PHP (.php)\n  \
      • Rust 2024 (.rs)\n\n\
    DETECTED SEMANTIC OPERATIONS:\n  \
      • MOVE     (↔) - Code block / function moved across files or locations\n  \
      • RENAME   (✎) - Entity renamed with structural shape preserved\n  \
      • MODIFY   (~) - Entity body modified\n  \
      • INSERT   (+) - New entity inserted\n  \
      • DELETE   (-) - Entity deleted\n\n\
    EXAMPLES:\n  \
      $ symtrace                              # Compare working directory against HEAD\n  \
      $ symtrace . HEAD                       # Compare commit_a against working directory\n  \
      $ symtrace . HEAD --staged              # Compare commit_a against staged index\n  \
      $ symtrace . HEAD~1 HEAD                # Compare two explicit commits\n  \
      $ symtrace -r /path/to/repo HEAD~1 HEAD # Compare commits in explicit repo path\n  \
      $ symtrace . HEAD~1 HEAD -p \"src/**/*.rs\"# Filter by path glob\n  \
      $ symtrace . HEAD~1 HEAD --format json  # Structured JSON output\n  \
      $ symtrace . HEAD~1 HEAD --logic-only   # Ignore comments & whitespace\n\n\
    CONFIGURATION:\n  \
      Loads configuration automatically from .symtracerc or symtrace.toml in the repository\n  \
      root or user home directory (~/.config/symtrace/symtrace.toml)."
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Path to local git repository (optional flag)
    #[arg(short = 'r', long = "repo")]
    pub repo_flag: Option<String>,

    /// First positional target (repo path or commit_a)
    pub arg1: Option<String>,

    /// Second positional target (commit_a or commit_b)
    pub arg2: Option<String>,

    /// Third positional target (commit_b)
    pub arg3: Option<String>,

    /// Diff staged index changes against commit_a
    #[arg(long, alias = "cached")]
    pub staged: bool,

    /// Ignore comments and whitespace-only changes
    #[arg(long)]
    pub logic_only: bool,

    /// Output structured JSON instead of formatted CLI text
    #[arg(long)]
    pub json: bool,

    /// Filter changed files matching glob pattern (e.g. "src/**/*.rs")
    #[arg(short = 'p', long = "path")]
    pub path_glob: Option<String>,

    /// Terminal color control: auto, always, never
    #[arg(long, default_value = "auto")]
    pub color: String,

    /// Disable automatic shell pager routing ($PAGER)
    #[arg(long)]
    pub no_pager: bool,

    /// Custom configuration file path (.symtracerc / symtrace.toml)
    #[arg(long)]
    pub config: Option<String>,

    /// Maximum file size in bytes before skipping (default: 5 MiB)
    #[arg(long, default_value_t = 5_242_880)]
    pub max_file_size: usize,

    /// Maximum AST nodes per file before skipping (default: 200,000)
    #[arg(long, default_value_t = 200_000)]
    pub max_ast_nodes: usize,

    /// Maximum parser recursion depth (default: 2,048)
    #[arg(long, default_value_t = 2_048)]
    pub max_recursion_depth: usize,

    /// Parse timeout in milliseconds, 0 to disable (default: 2,000)
    #[arg(long, default_value_t = 2_000)]
    pub parse_timeout_ms: u64,

    /// Disable incremental parsing (always do full parse)
    #[arg(long)]
    pub no_incremental: bool,

    /// Output high-level semantic summary table
    #[arg(short = 's', long)]
    pub stat: bool,

    /// Exit with code 1 if structural semantic changes exist
    #[arg(long)]
    pub check: bool,

    /// List only changed file paths containing structural changes
    #[arg(long)]
    pub name_only: bool,

    /// Specify output format: ansi, json, jsonl, markdown, html, sarif
    #[arg(long, default_value = "ansi")]
    pub format: String,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Native git diff driver mode (invoked by git diff)
    #[command(name = "git-diff-driver")]
    GitDiffDriver {
        /// File path
        path: String,
        /// Old file path / temporary file
        old_file: String,
        /// Old blob hex OID
        old_hex: String,
        /// Old file mode
        old_mode: String,
        /// New file path / temporary file
        new_file: String,
        /// New blob hex OID
        new_hex: String,
        /// New file mode
        new_mode: String,
    },

    /// Interactive TUI inspector mode
    #[command(name = "tui")]
    Tui {
        /// Target commit or ref (default: HEAD~1)
        #[arg(default_value = "HEAD~1")]
        commit_a: String,
        /// Target commit or ref (optional)
        commit_b: Option<String>,
    },

    /// Native 3-way AST semantic merge driver mode (invoked by git merge)
    #[command(name = "merge-driver")]
    MergeDriver {
        /// Base commit file (%O)
        base_file: String,
        /// Ours commit file (%A)
        ours_file: String,
        /// Theirs commit file (%B)
        theirs_file: String,
        /// Display file path (%P)
        display_path: String,
    },
}
