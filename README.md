<p align="center">
  <img src="https://raw.githubusercontent.com/symtrace/symtrace/main/vscode-symtrace/media/symtrace-banner.jpg" alt="symtrace banner" width="700">
</p>

# symtrace

A **deterministic semantic diff engine** written in Rust that compares Git commits using **AST-based structural analysis** instead of traditional line-based text diffs.

Where `git diff` shows you *lines that changed*, `symtrace` shows you *what semantically changed* - functions moved, classes deleted, variables renamed, code blocks inserted - at the AST node level, with zero false positives from formatting or comment edits.

> [!IMPORTANT]
> **Detailed Technical Specifications & Architecture:**
> For in-depth technical documentation covering the 5-phase AST matching algorithm, BLAKE3 node identity hashing, multi-file index graph, native 3-way AST merge driver, SARIF/HTML schemas, and security supply chain specs, please see [TECHNICAL_SPECIFICATIONS.md](TECHNICAL_SPECIFICATIONS.md).

## What Problem Does `symtrace` Solve?

### The Problem with Standard `git diff`

When you reformat your code, move a function down 50 lines, or rename a variable inside a function, standard `git diff` compares your code **line-by-line**. It marks whole sections as deleted and re-inserted:

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

### The `symtrace` Solution

`symtrace` parses code into an Abstract Syntax Tree (AST) - understanding real code constructs (functions, variables, classes, imports). It filters out whitespace, comments, and formatting tweaks to display **exact semantic operations**:

```text
━━━ src/user.rs
  [RENAME] variable 'user' renamed to 'account' (L12 → L85) [95% similarity]
  [MOVE]   function_item 'process_user' moved (L10 → L83) [100% similarity]
  ── Refactor Patterns ──
    ▸ Variable renamed 'user' -> 'account' inside 'process_user' (confidence: 100%)
```

## Understanding Output Operations (Cheatsheet)

When `symtrace` analyzes a diff, operations are classified into five primary types:

| Operation | Plain-English Description | Example Scenario |
| :--- | :--- | :--- |
| **`[MOVE]`** | A function, class, or method was moved to a new line or another file without logic changes. | Moving `fn parseConfig` to the bottom of the file or into `config.rs`. |
| **`[RENAME]`** | A variable, function, or struct was renamed across its scope. | Renaming parameter `user_id` to `account_id`. |
| **`[MODIFY]`** | The internal logic of an existing function or class was modified. | Adding an `if` validation check inside an existing method. |
| **`[INSERT]`** | A new function, struct, class, or code block was added. | Declaring a new helper function `fn validate_token`. |
| **`[DELETE]`** | A function, struct, class, or code block was removed. | Removing a deprecated legacy function. |

## Features (v0.5.0)

- **Understands Code Structure:** Sees true code changes like moved functions or renamed variables without getting confused by formatting tweaks or comment updates.
- **Adaptive Granularity:** Intelligently switches between micro-compact 1–3 line summaries (`--compact`) and full structural views, eliminating micro-commit noise overhead.
- **Cross-File Call Graph & Blast Radius:** Traces transitive caller impact up to depth 5 across file boundaries when signatures change.
- **Contract Violation & Safety Guard Alerts:** Detects removed null checks, deleted bounds guards, stripped mutex locks, or omitted resource cleanup.
- **Declarative AST Semantic Linter (`symtrace lint`):** Evaluates custom Tree-sitter `.scm` rules with CI severity thresholding.
- **LLM Context Optimization (`--format prompt`):** Ultra-dense serialization reducing token consumption by 80% for AI coding assistants.
- **Multi-Language Support:** Works seamlessly across 13 popular languages & formats: Rust, JavaScript, TypeScript, Python, Java, C, C++, Go, JSON, C#, Ruby, PHP, and Rust 2024.
- **Plugs Right Into Git:** Integrates directly with your existing Git workflow as a drop-in `git diff` replacement or native 3-way merge driver.
- **Interactive TUI Inspector (`symtrace tui`):** Zero-flicker terminal workspace with arrow-key controls to scroll files, inspect operations, and view line details.
- **White-Mode HTML & PDF Export:** Generates professional white-mode reports (`symtrace_report.html`) complete with a `Print / Save PDF` button and cryptographic BLAKE3 digital signatures.
- **Two-Tier CAS Caching & SIMD Acceleration:** Warm diff cache hits in $< 0.004$ ms and SIMD-vectorized token multiset similarity scoring.

## Supported Languages

| Language | Extensions | Key Entity Identifiers |
| :--- | :--- | :--- |
| **Rust / Rust 2024** | `.rs` | `function_item`, `struct_item`, `enum_item`, `impl_item`, `trait_item` |
| **JavaScript / JSX** | `.js`, `.jsx`, `.mjs`, `.cjs` | `function_declaration`, `class_declaration`, `method_definition` |
| **TypeScript / TSX** | `.ts`, `.tsx` | `function_declaration`, `class_declaration`, `interface_declaration`, `type_alias` |
| **Python** | `.py`, `.pyi` | `function_definition`, `class_definition` |
| **Java** | `.java` | `method_declaration`, `class_declaration`, `interface_declaration` |
| **C** | `.c`, `.h` | `function_definition`, `struct_specifier`, `enum_specifier`, `type_definition` |
| **C++** | `.cpp`, `.hpp`, `.cc`, `.cxx` | `function_definition`, `class_specifier`, `namespace_definition` |
| **Go** | `.go` | `function_declaration`, `method_declaration`, `type_declaration` |
| **JSON** | `.json`, `.jsonc` | `pair`, `object`, `array` |
| **C#** | `.cs` | `method_declaration`, `class_declaration`, `interface_declaration` |
| **Ruby** | `.rb` | `method`, `class`, `module` |
| **PHP** | `.php` | `method_declaration`, `class_declaration`, `function_definition` |

## Installation

### Linux / macOS (Shell Script)

```bash
curl -fsSL https://raw.githubusercontent.com/symtrace/symtrace/main/install.sh | bash
```

### Windows (PowerShell Script)

```powershell
iwr -useb https://raw.githubusercontent.com/symtrace/symtrace/main/install.ps1 | iex
```

## Command Usage & Examples

### 1. Basic Diff Execution & Adaptive Granularity

```bash
# Compare uncommitted working tree changes against HEAD in current directory
symtrace

# Compare staged changes (git add) against HEAD
symtrace . HEAD --staged

# Compare two specific commits or branches
symtrace . main feature-branch
symtrace . HEAD~1 HEAD

# Force micro-compact 1-3 line inline token summaries (ideal for small edits)
symtrace . HEAD~1 HEAD --compact

# Force full structural headers and diagnostic banners
symtrace . HEAD~1 HEAD --full-headers
```

### 2. AI / LLM Prompt Context Exporter (`--format prompt`)

```bash
# Generate ultra-dense structural context optimized for LLMs (Gemini, Claude, GPT)
# Consumes ~80% fewer prompt tokens than unified diffs while exposing contract violations
symtrace . HEAD~1 HEAD --format prompt
```

### 3. Declarative AST Semantic Linter (`symtrace lint`)

```bash
# Evaluate custom Tree-sitter .scm rules in the repository
symtrace lint

# Lint a specific file or directory
symtrace lint src/server.rs

# Enforce strict CI threshold (fail if warnings > 0)
symtrace lint . --max-warnings 0

# Emit linter diagnostics as JSON or SARIF for automated CI pipelines
symtrace lint . --format sarif
```

### 4. Interactive Terminal UI (TUI)

```bash
# Launch interactive zero-flicker terminal inspector with arrow-key controls
symtrace tui HEAD~1 HEAD
```

### 5. File & Path Filtering

```bash
# Restrict AST diffing to Rust source files in src/
symtrace . HEAD~1 HEAD --path "src/**/*.rs"

# Restrict AST diffing to JavaScript/TypeScript files
symtrace . HEAD~1 HEAD -p "**/*.{js,ts}"
```

### 6. Ignoring Comments & Whitespace

```bash
# Evaluate strictly logic-only AST nodes (ignores all comments & whitespace)
symtrace . HEAD~1 HEAD --logic-only
```

### 7. Multi-Format Reporting & Export

```bash
# Generate a White-Mode HTML report (symtrace_report.html) with digital signatures
symtrace . HEAD~1 HEAD --format html

# Emit high-level semantic summary table
symtrace . HEAD~1 HEAD --stat

# Output machine-readable JSON for CI pipelines
symtrace . HEAD~1 HEAD --format json

# Output OASIS SARIF v2.1.0 JSON schema for GitHub Code Scanning
symtrace . HEAD~1 HEAD --format sarif
```

### 8. Drop-in Git Diff Driver Integration

Configure `symtrace` as your default `GIT_EXTERNAL_DIFF`:

```bash
# Set symtrace git-diff-driver for a single git invocation
GIT_EXTERNAL_DIFF="symtrace git-diff-driver" git diff HEAD~1

# Or configure globally in git
git config --global diff.external "symtrace git-diff-driver"
```

## Subcommands & Integration

### Declarative AST Linter (`symtrace lint`)

Run Tree-sitter query rules defined in `.symtrace/queries/*.scm`:

```scheme
;; .symtrace/queries/no_unwrap.scm
;; @id no-unwrap-in-production
;; @severity ERROR
;; @message Avoid calling unwrap() in production code; handle Result gracefully.
(call_expression
  function: (field_expression field: (field_identifier) @method (#eq? @method "unwrap")))
```

### Native 3-Way AST Merge Driver (`symtrace merge-driver`)

Configure `symtrace` as a native 3-way merge driver in `.gitconfig`:

```bash
git config merge.symtrace.name "symtrace 3-Way AST Merge Driver"
git config merge.symtrace.driver "symtrace merge-driver %O %A %B %P"
```

### Flexible Export Formats (`--format <FMT>`)

Supports `--format ansi`, `--format json`, `--format jsonl`, `--format markdown`, `--format html`, `--format prompt`, and `--format sarif`.

## Frequently Asked Questions (FAQ)

### Does `symtrace` replace standard Git commands?

No. `symtrace` operates alongside Git as a non-destructive analysis tool. Standard `git diff` remains fully functional. `symtrace` provides a higher-level structural perspective during code reviews, CI linting, and complex refactoring audits.

### Is source code processed locally or sent to a cloud server?

All analysis runs 100% locally on your machine. `symtrace` is completely offline, collects zero telemetry, and performs zero network requests.

### How do I use `symtrace` inside VS Code?

Install the **symtrace for VS Code** extension from the VS Code Marketplace. It integrates a dedicated sidebar panel, inline decorations, and side-by-side diff views directly within your editor.

## Additional Documentation

- [TECHNICAL_SPECIFICATIONS.md](TECHNICAL_SPECIFICATIONS.md) - Comprehensive technical reference.
- [SECURITY.md](SECURITY.md) - Security policy & supply chain guarantees.
- [BENCHMARKS.md](BENCHMARKS.md) - Performance benchmark reports.
- [CHANGELOG.md](CHANGELOG.md) - Version release history & changelog.
- [BENCHMARKING_PROTOCOL.md](BENCHMARKING_PROTOCOL.md) - Research benchmarking protocol & plan.
- [BENCHMARKS_RESULTS.md](BENCHMARKS_RESULTS.md) - Research benchmarking results.

## License

[MIT License](LICENSE) © 2026 Jash Thakkar.
