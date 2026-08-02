use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "symtrace")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "Deterministic semantic diff engine using AST-based structural analysis")]
#[command(
    long_about = "symtrace is a deterministic semantic diff engine written in Rust that compares Git \
    commits, staged index changes, or working tree modifications using AST-based structural analysis \
    instead of line-based text diffs.\n\n\
    SUPPORTED LANGUAGES (9):\n  \
      • Rust (.rs)\n  \
      • JavaScript (.js, .jsx, .mjs, .cjs)\n  \
      • TypeScript (.ts, .tsx)\n  \
      • Python (.py, .pyi)\n  \
      • Java (.java)\n  \
      • C (.c, .h)\n  \
      • C++ (.cpp, .hpp, .cc, .cxx, .h++)\n  \
      • Go (.go)\n  \
      • JSON (.json, .jsonc)\n\n\
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
      $ symtrace . HEAD~1 HEAD -p \"src/**/*.rs\"# Filter by path glob\n  \
      $ symtrace . HEAD~1 HEAD --json         # Structured JSON output\n  \
      $ symtrace . HEAD~1 HEAD --logic-only   # Ignore comments & whitespace\n\n\
    CONFIGURATION:\n  \
      Loads configuration automatically from .symtracerc or symtrace.toml in the repository\n  \
      root or user home directory (~/.config/symtrace/symtrace.toml)."
)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Path to local git repository (defaults to current directory if omitted)
    #[arg(default_value = ".")]
    pub repo_path: String,

    /// Older commit reference (hash, HEAD~1, branch, tag, etc., defaults to HEAD~1)
    #[arg(default_value = "HEAD~1")]
    pub commit_a: String,

    /// Newer commit reference (hash, HEAD, branch, tag, etc., optional)
    pub commit_b: Option<String>,

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
}
