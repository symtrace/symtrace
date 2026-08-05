<p align="center">
  <img src="vscode-symtrace/media/symtrace-logo.png" alt="symtrace logo" width="400">
</p>

# symtrace

A **deterministic semantic diff engine** written in Rust that compares Git commits using **AST-based structural analysis** instead of traditional line-based text diff.

Where `git diff` shows you *lines that changed*, `symtrace` shows you *what semantically changed* — functions moved, classes deleted, variables renamed, code blocks inserted — at the AST node level, with no false positives from formatting or comment edits.

### Example: Traditional `git diff` vs. `symtrace`

When moving a function and renaming a variable inside it:

**Traditional `git diff` (Line-based deletion & insertion):**

```diff
- fn process_user(id: u64) -> User {
-     let user = fetch_user(id);
-     user
- }
...
+ fn process_user(user_id: u64) -> User {
+     let account = fetch_user(user_id);
+     account
+ }
```

**`symtrace` (AST-based semantic understanding):**

```
━━━ src/user.rs
  ✎ [RENAME] variable 'user' renamed to 'account' (L12 → L85) [95% similarity, low]
  ↔ [MOVE]   function_item 'process_user' moved (L10 → L83) [100% similarity, low]
  ── Refactor Patterns ──
    ▸ Variable renamed 'user' ➔ 'account' inside 'process_user' (confidence: 100%)
```

## Features (v0.3.0)

### Key Highlights (Beginner-Friendly)

- **Understands Code Structure** — Sees true code changes like moved functions or renamed variables without getting confused by formatting tweaks or comment updates.
- **Multi-Language Support** — Works seamlessly across 9 popular languages/formats: Rust, JavaScript, TypeScript, Python, Java, C, C++, Go, and JSON.
- **Plugs Right Into Git** — Integrates directly with your existing Git workflow as a drop-in `git diff` replacement.
- **Detects Refactoring Patterns** — Automatically identifies structural refactors (like function extraction or renames) and categorizes commit types.
- **Blazing Fast & Offline** — Built in high-performance Rust with intelligent caching, operating entirely locally with zero network calls.
- **Automation Ready** — Produces structured JSON output for easy integration into CI/CD pipelines and custom tools.

### Technical Specifications

- **Semantic Operations** — MOVE, RENAME, MODIFY, INSERT, DELETE detected at the AST node level
- **Expanded Language Support** — 9 first-class languages/formats: Rust, JavaScript, TypeScript, Python, Java, C, C++, Go, and JSON
- **Native Git Diff Driver** — Direct integration as a `git diff` driver via `git-diff-driver` subcommand
- **Interactive Shell Pager** — Automatic piping to `$GIT_PAGER` / `$PAGER` (`less -RFX`) for interactive TTY execution
- **Path Glob Filtering** — Restrict diffing to specific patterns using `--path <GLOB>` (`-p`)
- **Flexible Commit Resolution** — Positional arguments default gracefully (repo path defaults to `.`, `commit_a` to `HEAD~1`, `commit_b` to working tree); supports `--staged` / `--cached`
- **Config Loader** — Hierarchical `.symtracerc` / `symtrace.toml` configuration loader
- **ANSI Color Controls** — `--color <auto|always|never>` respecting standard `NO_COLOR` environment variables
- **Dual-Path Git Rename Tracking** — Retains both `old_path` and `new_path` across file rename events
- **5-Phase Matching Algorithm** — Exact hash match → structural match → similarity scoring → leftovers
- **4-Hash BLAKE3 Node Identity** — Structural, content, identity, and context hashes per node (limits-aware cache keying)
- **Bounded LRU Caches** — In-memory LRU tree cache (500 capacity) + external versioned AST cache
- **Refactor Pattern & Symbol Tracking** — Cross-file symbol movement, rename tracking, and method extraction detection
- **Commit Classification** — Automatically labels commits (feature, bugfix, refactor, cleanup, formatting_only, etc.)
- **Machine-Readable Output** — Structured `--json` format for CI/CD pipelines
- **Rigorous Quality & Testing** — Property-based testing (`proptest`), differential testing, fuzzing (`cargo-fuzz`), and 166 passing tests
- **Security & Provenance** — Safe Rust enforced (`unsafe_code = "deny"`), keyless Cosign OIDC signing, SPDX SBOM, and GitHub Artifact Attestations

## Supported Languages

| Language | Extensions | Key Entity Identifiers |
| ------------ | ------------------------------------- | ------------------------ |
| **Rust** | `.rs` | `function_item`, `struct_item`, `enum_item`, `impl_item`, `trait_item` |
| **JavaScript** | `.js`, `.jsx`, `.mjs`, `.cjs` | `function_declaration`, `class_declaration`, `method_definition` |
| **TypeScript** | `.ts`, `.tsx` | `function_declaration`, `class_declaration`, `interface_declaration`, `type_alias_declaration` |
| **Python** | `.py`, `.pyi` | `function_definition`, `class_definition` |
| **Java** | `.java` | `method_declaration`, `class_declaration`, `interface_declaration` |
| **C** | `.c`, `.h` | `function_definition`, `struct_specifier`, `enum_specifier`, `type_definition` |
| **C++** | `.cpp`, `.hpp`, `.cc`, `.cxx`, `.h++` | `function_definition`, `class_specifier`, `namespace_definition`, `template_declaration` |
| **Go** | `.go` | `function_declaration`, `method_declaration`, `type_declaration`, `type_spec` |
| **JSON** | `.json`, `.jsonc` | `pair`, `object`, `array` |

## Quick Start

```bash
# Compare working directory against HEAD (in current directory)
symtrace

# Compare specific commit against working directory
symtrace . HEAD

# Compare staged index against HEAD
symtrace . HEAD --staged

# Compare two commits
symtrace /path/to/repo a1b2c3d 9f8e7d6

# Filter diff by file path glob
symtrace . HEAD~1 HEAD -p "src/**/*.rs"

# Emit machine-readable JSON
symtrace . HEAD~1 HEAD --json

# Ignore comment/whitespace changes
symtrace . HEAD~1 HEAD --logic-only
```

## Installation

### Linux / macOS (Shell Script)

```bash
curl -fsSL https://raw.githubusercontent.com/JashT14/symtrace/main/install.sh | bash
```

### Windows (PowerShell Script)

```powershell
iwr -useb https://raw.githubusercontent.com/JashT14/symtrace/main/install.ps1 | iex
```

## Usage & Configuration

```
symtrace [REPO_PATH] [COMMIT_A] [COMMIT_B] [OPTIONS]
```

### Arguments

| Argument      | Default | Description |
|---------------|---------|-------------|
| `REPO_PATH`   | `.`     | Path to local Git repository |
| `COMMIT_A`    | `HEAD~1`| Older commit ref, tag, or branch |
| `COMMIT_B`    | Working Tree | Newer commit ref, tag, or branch (optional) |

### Options

| Flag | Short | Default | Description |
| ------ | ------- | --------- | ------------- |
| `--staged` / `--cached` | | off | Compare staged index against `COMMIT_A` |
| `--path <GLOB>` | `-p` | | Filter files using path glob pattern (e.g. `"src/**/*.rs"`) |
| `--color <WHEN>` | | `auto` | Terminal color controls (`auto`, `always`, `never`) |
| `--no-pager` | | off | Disable piping output to shell pager (`$PAGER`) |
| `--logic-only` | | off | Ignore comments and whitespace-only nodes |
| `--json` | | off | Emit machine-readable JSON output |
| `--no-incremental` | | off | Disable incremental AST parsing |
| `--max-file-size <BYTES>` | | 5242880 | Skip files larger than specified bytes (5 MiB default) |
| `--max-ast-nodes <N>` | | 200000 | Skip files with more AST nodes than specified |
| `--max-recursion-depth <N>` | | 2048 | Maximum AST parser recursion depth |
| `--parse-timeout-ms <MS>` | | 2000 | Per-file parse timeout in ms (0 = disabled) |
| `--help` | `-h` | | Print help |
| `--version` | `-V` | | Print version |

### Subcommands

#### Native Git Diff Driver (`git-diff-driver`)

Configure `symtrace` as a native `git diff` driver for specific file types or entire repositories:

```bash
# Configure Git driver command
git config diff.symtrace.command "symtrace git-diff-driver"

# Set file attributes in .gitattributes
echo "*.rs diff=symtrace" >> .gitattributes
echo "*.js diff=symtrace" >> .gitattributes
```

Now running standard `git diff` automatically renders semantic AST diffs.

### Configuration File (`.symtracerc` / `symtrace.toml`)

`symtrace` automatically loads configuration from repository root `.symtracerc` or `symtrace.toml`, or user home config (`~/.config/symtrace/symtrace.toml`). Precedence order: **CLI Flags > Repository Config > User Config > Defaults**.

Sample configuration file:

```toml
[default]
logic_only = false
json = false
no_incremental = false
no_pager = false

[limits]
max_file_size = 10485760     # 10 MiB
max_ast_nodes = 500000       # 500,000 nodes
max_recursion_depth = 2048
parse_timeout_ms = 3000

[output]
color = "auto"
```

## How It Works

```
Repository Target Resolution ──► Dual-Path Git File Changes ──► Path Glob Filtering
 (Commits / Index / Work)        (Old & New Path Pairs)          (--path "src/**/*.rs")
           │                                                               │
           ▼                                                               ▼
 Bounded TreeCache LRU ◄────── Incremental AST Parsing ◄───── Versioned AST Cache
  (Cap: 500 trees)             (BLAKE3 Hash Reuse)           (Blob + Limits Key)
           │
           ▼
 5-Phase AST Matching ───────► Deep BFS Symbol Tracking ─────► Output Formats
 (Parallel via Rayon)          (5-Level Name Resolution)     (ANSI / Pager / JSON)
```

### Architecture Overview

| Module | Responsibility |
| -------- | ---------------- |
| `main.rs` | Pipeline orchestration, target resolution, subcommand handling, timing |
| `cli.rs` | CLI argument definitions (`clap`), positional defaults, flags |
| `git_layer.rs` | Repository access (`libgit2`), dual-path rename extraction, index/workdir diffs |
| `language.rs` | Extension matching for 9 supported languages/formats |
| `ast_builder.rs` | Tree-sitter parsing, arena allocation (`bumpalo`), limits verification |
| `ast_cache.rs` | Two-tier AST cache (in-memory LRU + versioned on-disk storage with limits hash) |
| `incremental_parse.rs` | Bounded `TreeCache` LRU (500 capacity), minimal edit computation |
| `node_identity.rs` | 4-hash BLAKE3 identity computation per node |
| `tree_diff.rs` | Parallel 5-phase AST node matching algorithm |
| `semantic_similarity.rs` | Structural, token, and complexity similarity calculation |
| `symbol_tracking.rs` | Deep BFS symbol extraction & cross-file movement tracking |
| `refactor_detection.rs` | Refactor pattern detection (extract method, move, rename) |
| `commit_classification.rs` | Commit auto-classification (feature, refactor, cleanup, etc.) |
| `pager.rs` | Interactive terminal detection and `$PAGER` process routing (`less -RFX`) |
| `config.rs` | Hierarchical `.symtracerc` / `symtrace.toml` TOML config loader |
| `output.rs` | ANSI color renderer and structured JSON formatter |
| `types.rs` | Shared domain data structures and `FileChange` dual-path representations |

## Performance & Empirical Benchmarks

Tested on a release build (`LLVM -O3`, LTO enabled) against the local `express` repository (`d:\rust_playground\express`):

| Scenario | Mode / Command | Processed Files | AST Nodes | Parse Time | Diff Time | Total Time | Speedup Factor |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Cold AST Parse** | `symtrace express HEAD~1 HEAD` (Cold) | 2 | 6,011 | 32.21 ms | 0.94 ms | 86.77 ms | Baseline |
| **Warm Cache Hit** | `symtrace express HEAD~1 HEAD` (Warm) | 2 | 6,011 | 3.77 ms | 1.24 ms | 22.21 ms | **3.91× faster** |
| **JSON Output** | `symtrace express HEAD~1 HEAD --json` | 2 | 6,011 | 3.66 ms | 1.00 ms | 18.38 ms | **4.72× faster** |
| **Working Tree Diff** | `symtrace express HEAD` (Single Commit) | 0 | 0 | 0.01 ms | 0.00 ms | 142.88 ms | N/A |
| **Full Test Suite** | `cargo test --all` (166 tests) | N/A | ~65,000 | N/A | N/A | **90.00 ms** (0.09s) | **166 tests / 90ms** |

## Security & Supply Chain

- **Zero Unsafe Rust** — Enforced via `#![deny(unsafe_code)]` in `Cargo.toml`.
- **Zero Network Access** — Fully offline, no telemetry, no HTTP/TCP dependencies.
- **Process Isolation** — Shell pager executed directly via explicit argument vectors (no `sh -c` / `cmd.exe /c` shell evaluation).
- **Path Traversal Protection** — Repository paths canonicalized with directory verification prior to Git access.
- **Resource Limits & Fuzzing** — Hard bounds on file size, node count, recursion depth, and parse timeouts fuzzed via `cargo-fuzz`.
- **Cosign & Provenance** — Release assets signed keylessly via Sigstore/Cosign OIDC with SPDX SBOM (`symtrace.spdx.json`) and GitHub Artifact Attestations.

See [SECURITY.md](SECURITY.md) for full security documentation.

## Dependencies

| Crate | Version | Purpose |
| ------- | --------- | --------- |
| `clap` | `=4.5.60` | CLI argument parsing |
| `git2` | `=0.19.0` | libgit2 bindings for Git repository access |
| `tree-sitter` | `=0.25.10` | Parser framework |
| `tree-sitter-rust` | `=0.24.0` | Rust language grammar |
| `tree-sitter-javascript` | `=0.25.0` | JavaScript language grammar |
| `tree-sitter-typescript` | `=0.23.2` | TypeScript language grammar |
| `tree-sitter-python` | `=0.25.0` | Python language grammar |
| `tree-sitter-java` | `=0.23.5` | Java language grammar |
| `tree-sitter-c` | `=0.23.4` | C language grammar |
| `tree-sitter-cpp` | `=0.23.4` | C++ language grammar |
| `tree-sitter-go` | `=0.23.4` | Go language grammar |
| `tree-sitter-json` | `=0.24.8` | JSON language grammar |
| `blake3` | `=1.8.3` | SIMD-optimized BLAKE3 hashing for node identity |
| `serde` / `serde_json` | `=1.0.228` / `=1.0.149` | Serde JSON serialization |
| `bincode` | `=1.3.3` | Bounded binary serialization for AST cache |
| `rayon` | `=1.11.0` | Multi-threaded parallel parsing & diffing |
| `lru` | `=0.12.5` | In-memory LRU cache |
| `bumpalo` | `=3.20.2` | Arena allocator for zero-overhead AST construction |
| `globset` | `=0.4.15` | Glob pattern matching for path filtering |
| `toml` | `=0.8.23` | TOML parser for `.symtracerc` configuration loader |
| `colored` | `=2.2.0` | ANSI terminal styling |
| `anyhow` | `=1.0.102` | Error handling |
| `proptest` | `=1.5.0` | (dev-dependency) Property-based testing framework |

All dependencies are strictly pinned (`=x.y.z`). See [Cargo.toml](Cargo.toml) for details.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines.

## License

[MIT License](LICENSE) © 2026 Jash Thakkar.
