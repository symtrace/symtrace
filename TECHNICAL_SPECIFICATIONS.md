# `symtrace` v0.4.0 - Technical Specifications & Architecture Reference

**Project:** `symtrace` - Deterministic Semantic Diff Engine  
**Version:** `v0.4.0`  
**License:** MIT  

## 1. Architecture Overview

`symtrace` replaces traditional line-based `git diff` output with **AST-based structural analysis**, detecting semantic changes (`MOVE`, `RENAME`, `MODIFY`, `INSERT`, `DELETE`) across files while filtering out whitespace, formatting, and comment noise.

```
Repository Target Resolution ──► Dual-Path Git File Changes ──► Path Glob Filtering
 (Commits / Index / Work)        (Old & New Path Pairs)          (--path "src/**/*.rs")
           │                                                               │
           ▼                                                               ▼
 Bounded TreeCache LRU ◄────── Incremental AST Parsing ◄───── Versioned AST Cache
  (Cap: 500 trees)             (BLAKE3 Hash Reuse)           (Zero-Copy Blob OID)
           │
           ▼
 5-Phase AST Matching ───────► Global Multi-File AST Index ──► Output Formats
 (Parallel via Rayon)          (O(N log N) Graph Tracking)    (ANSI / JSON / HTML / SARIF)
```

## 2. Module Responsibilities Matrix

| Module | File Path | Responsibility |
| :--- | :--- | :--- |
| **Main Orchestrator** | [src/main.rs](src/main.rs) | Pipeline orchestration, positional argument normalization, subcommand routing (`Tui`, `MergeDriver`), timing. |
| **CLI Parser** | [src/cli.rs](src/cli.rs) | CLI argument definitions (`clap`), positional defaults, flags (`--stat`, `--check`, `--name-only`, `--format`). |
| **Git Integration** | [src/git_layer.rs](src/git_layer.rs) | Repository access (`libgit2`), thread-local handle pooling (`SharedBlobReader`), `diff.find_similar()`, and `Repository::discover`. |
| **Language Support** | [src/language.rs](src/language.rs) | Extension matching for 13 supported languages (`Rust`, `JS`, `TS`, `Python`, `Java`, `C`, `C++`, `Go`, `JSON`, `C#`, `Ruby`, `PHP`, `Rust 2024`). |
| **AST Builder** | [src/ast_builder.rs](src/ast_builder.rs) | Tree-sitter parsing, `BumpaloRecycler` arena pooling, parser recycling, and bounds verification. |
| **Zero-Copy Cache** | [src/ast_cache.rs](src/ast_cache.rs) | Zero-copy OID AST cache (in-memory LRU + versioned on-disk Bincode storage with limits hash). |
| **Incremental Parse** | [src/incremental_parse.rs](src/incremental_parse.rs) | Bounded `TreeCache` LRU (500 capacity), minimal edit computation. |
| **Node Identity** | [src/node_identity.rs](src/node_identity.rs) | 4-hash BLAKE3 identity computation per node (structural, content, identity, context). |
| **Tree Diff Engine** | [src/tree_diff.rs](src/tree_diff.rs) | Parallel 5-phase AST matching algorithm, `GlobalNodeIndex`, subtree windowing for files >1 MiB. |
| **Semantic Similarity** | [src/semantic_similarity.rs](src/semantic_similarity.rs) | Structural, token, and complexity similarity calculation algorithms. |
| **Symbol Tracking** | [src/symbol_tracking.rs](src/symbol_tracking.rs) | Deep BFS symbol extraction & cross-file movement tracking. |
| **Refactor Engine** | [src/refactor_detection.rs](src/refactor_detection.rs) | Refactor pattern detection (extract method, move, rename variable). |
| **Query DSL Engine** | [src/query_dsl.rs](src/query_dsl.rs) | Custom Tree-Sitter `.scm` query loader and rule evaluation engine (`.symtrace/queries/`). |
| **Commit Classification** | [src/commit_classification.rs](src/commit_classification.rs) | Commit auto-classification (feature, refactor, bugfix, cleanup, formatting_only). |
| **TUI Inspector** | [src/tui.rs](src/tui.rs) | Event-driven zero-flicker crossterm interactive TUI inspector with arrow-key navigation. |
| **3-Way Merge Driver** | [src/merge_driver.rs](src/merge_driver.rs) | Native 3-way AST semantic merge driver (`git config merge.symtrace.driver`). |
| **Terminal Pager** | [src/pager.rs](src/pager.rs) | Interactive terminal detection and `$PAGER` process routing (`less -RFX`). |
| **Config Loader** | [src/config.rs](src/config.rs) | Hierarchical `.symtracerc` / `symtrace.toml` TOML configuration loader. |
| **Multi-Format Output** | [src/output.rs](src/output.rs) | Multi-format output engines (ANSI, JSON, JSONL, Markdown, White-Mode Signed HTML/PDF, SARIF v2.1.0). |
| **Shared Domain Types** | [src/types.rs](src/types.rs) | Shared domain data structures and camelCase serde representations (`#[serde(rename_all = "camelCase")]`). |

## 3. 5-Phase AST Node Matching Engine

The core diff engine ([src/tree_diff.rs](src/tree_diff.rs)) correlates nodes between old (Commit A) and new (Commit B) AST trees using a deterministic 5-phase matching pipeline:

1. **Phase 1 - Exact BLAKE3 Identity Match ($O(1)$):**
   Matches nodes with identical `structural_hash` and `content_hash` using a HashMap lookup index. Unchanged subtrees are matched instantly without deep comparison.
2. **Phase 2 - Structural Shape Equivalence Match:**
   Matches nodes sharing identical AST structural tree shapes (`structural_hash`), ignoring identifier or variable name modifications.
3. **Phase 3 - Kind & Symbol Name Correlator:**
   Correlates declaration nodes sharing the same AST `kind` (e.g., `function_item`, `class_declaration`) and extracted symbol `name` within modified scope containers.
4. **Phase 4 - BLAKE3 Identity Fallback Match:**
   Correlates refactored function bodies using BLAKE3 `identity_hash` (structural hash combined with normalized symbol identifier strings).
5. **Phase 5 - Multi-Dimensional Similarity Scoring:**
   Evaluates remaining unmatched node pairs using weighted Jaccard similarity across token frequency vectors, structural child AST node type distributions, and cyclomatic complexity deltas ([src/semantic_similarity.rs](src/semantic_similarity.rs)).

## 4. Cryptographic BLAKE3 Node Identity Scheme

Each AST node ([src/node_identity.rs](src/node_identity.rs)) computes four distinct 32-byte BLAKE3 cryptographic hashes to establish identity:

- **`structural_hash`:** Hashes node `kind` and child subtree structural arrangement (ignores token identifiers, variable names, and comments).
- **`content_hash`:** Hashes exact raw UTF-8 literal content for leaf nodes.
- **`identity_hash`:** Combines `structural_hash` with normalized symbol identifier strings to recognize refactored declaration bodies.
- **`context_hash`:** Incorporates parent node `kind` and immediate sibling AST context to enforce strict lexical scope resolution.

### Normalization Rules

- **Comment Filtering:** In `--logic-only` mode, comment nodes are replaced with a static placeholder (`__COMMENT__`) to prevent comment edits from breaking structural hashes.
- **Whitespace Stripping:** Formatting variations (spaces, tabs, newlines) are stripped prior to hashing.

## 5. Incremental Parsing & Arena Allocator Pool

- **Minimal Byte Edit Computation ([src/incremental_parse.rs](src/incremental_parse.rs)):**
  `compute_edit` calculates minimal byte edit ranges (`InputEdit`) between source versions. `edited_tree.edit(&edit)` informs Tree-sitter's parser to reuse unchanged AST subtrees during re-parsing.
- **Arena Memory Recycling ([src/ast_builder.rs](src/ast_builder.rs)):**
  `BumpaloRecycler` maintains thread-local `bumpalo::Bump` arenas. AST construction temporaries are allocated inside the arena and freed in $O(1)$ time per file via `bump.reset()`, avoiding repeated heap allocations across worker threads.

## 6. Global Multi-File Graph Index & Symbol Tracking

- **Unified Repository Graph Index ([src/tree_diff.rs](src/tree_diff.rs)):**
  `GlobalNodeIndex` maps symbol declarations and usage sites across all repository files in a single pass ($O(N \log N)$ complexity), eliminating $O(F \times N^2)$ post-pass scanning.
- **Deep BFS Symbol Extraction ([src/symbol_tracking.rs](src/symbol_tracking.rs)):**
  Uses a 5-level Breadth-First Search (BFS) queue to extract symbol declarations across files, tracking cross-file `MOVE` and `RENAME` events.

## 7. Refactor Pattern Recognition & Intent Classifier

- **Refactor Pattern Engine ([src/refactor_detection.rs](src/refactor_detection.rs)):**
  - **Extract Method:** Detects new function insertions accompanied by corresponding call site deletions or function modifications.
  - **Move Method:** Identifies function declarations moved across files or scope boundaries.
  - **Rename Variable:** Correlates variable renames with identical structural usage patterns.
- **Commit Intent Classification ([src/commit_classification.rs](src/commit_classification.rs)):**
  Analyzes operation proportions to label commits as `feature`, `bugfix`, `refactor`, `cleanup`, `formatting_only`, or `mixed`.

## 8. Declarative Query DSL Engine

- **Tree-Sitter Query Loader ([src/query_dsl.rs](src/query_dsl.rs)):**
  `QueryEngine` loads custom Tree-sitter S-expression query rules (`.scm`) from `.symtrace/queries/`.
- **Rule Evaluation:**
  Evaluates custom query patterns against parsed AST nodes during diffing and attaches domain-specific alert metadata to output operations.

## 9. Subtree Windowing & Oversized File Optimization

When processing large source code files exceeding `OVERSIZED_FILE_THRESHOLD_BYTES` (`1_048_576` bytes / 1 MiB):

- `symtrace` extracts modified line hunks from Git diff deltas.
- `collect_significant_nodes_windowed` filters node collection strictly to subtrees overlapping modified line ranges.
- Reduces node matching complexity from $O(N^2)$ over the full file to $O(\text{hunk\_size})$, lowering execution time on 10 MiB files from 420 ms to **14 ms** (30× speedup).

## 10. Zero-Copy Cache & Disk Storage Architecture

- **In-Memory LRU Cache:** Maintains up to 500 Tree-sitter `Tree` objects in memory ([src/incremental_parse.rs](src/incremental_parse.rs)).
- **On-Disk Versioned Storage:** Persists serialized AST structures using Bincode under `$XDG_CACHE_HOME/symtrace` or `%LOCALAPPDATA%\symtrace` ([src/ast_cache.rs](src/ast_cache.rs)).
- **Cache Key Security:** Keys entries using Git Blob OID hex strings combined with a 64-bit `limits_hash` (derived from `ParserLimits`). Altering CLI parser limits automatically invalidates outdated cached entries.

## 11. Interactive TUI & Terminal Pager Architecture

- **Safe Terminal Pager Routing ([src/pager.rs](src/pager.rs)):**
  Spawns `$PAGER` / `$GIT_PAGER` (`less -RFX`) via explicit argument vectors (`std::process::Command::new`), bypassing shell evaluation (`sh -c` / `cmd.exe /c`).
- **Zero-Flicker Event-Driven TUI ([src/tui.rs](src/tui.rs)):**
  Built using `crossterm` raw terminal mode. Screen redraws execute exclusively on user input events (`crossterm::event::read()`). Terminal state is cleanly restored upon exit (`LeaveAlternateScreen`, `disable_raw_mode`).

## 12. Hierarchical Configuration Precedence & Resource Guardrails

- **Config Precedence ([src/config.rs](src/config.rs)):**
  Configuration is resolved in order: **CLI Flags > Repository Config (`.symtracerc` / `symtrace.toml`) > User Config (`~/.config/symtrace/symtrace.toml`) > Hardcoded Defaults**.
- **Resource Guardrail Defaults ([src/cli.rs](src/cli.rs)):**
  - `max_file_size`: 5 MiB (`5,242,880` bytes)
  - `max_ast_nodes`: 200,000 nodes
  - `max_recursion_depth`: 2,048 levels
  - `parse_timeout_ms`: 2,000 ms

## 13. Output Formats & Report Cryptographic Signing

| Format | Standard / Specification | Output Purpose |
| :--- | :--- | :--- |
| **ANSI** | Terminal ANSI escape styling | Interactive TTY terminal inspection |
| **JSON** | Structured camelCase JSON (`#[serde(rename_all = "camelCase")]`) | CI/CD automation & API integration |
| **JSONL** | Line-delimited JSON objects | Log streaming & analytics |
| **Markdown** | GitHub Flavored Markdown (GFM) tables | Pull Request comments |
| **HTML** | Standalone white-mode signed HTML (`symtrace_report.html`) | Executive reports & browser viewing |
| **SARIF** | OASIS SARIF v2.1.0 JSON schema | GitHub Code Scanning & static analysis |

### Report Cryptographic Signing & PDF Export

- **BLAKE3 Report Signature:** Calculates a 64-character BLAKE3 hash over serialized report bytes and embeds a `DIGITAL AUDIT SIGNATURE [VERIFIED]` badge ([src/output.rs](src/output.rs)).
- **PDF Layout Formatting:** Includes an embedded `Print / Save PDF` button (`<button onclick="window.print()">`) formatted with `@media print` CSS rules.

## 14. Native 3-Way AST Merge Driver Specs

Configure `symtrace` as a native 3-way Git merge driver ([src/merge_driver.rs](src/merge_driver.rs)):

```bash
git config merge.symtrace.name "SymTrace 3-Way AST Merge Driver"
git config merge.symtrace.driver "symtrace merge-driver %O %A %B %P"
```

Parses Base (`%O`), Ours (`%A`), and Theirs (`%B`) versions, automatically merging non-conflicting AST node mutations and writing camelCase conflict markers (`<<<<<<< Ours: fn processUser`) for overlapping structural logic conflicts.

## 15. Empirical Performance Benchmarks

| Scenario | Mode / Command | Files | AST Nodes | Parse Time | Diff Time | Total Time | Speedup |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Cold AST Parse** | `symtrace express HEAD~1 HEAD` (Cold) | 2 | 6,011 | 32.21 ms | 0.94 ms | 86.77 ms | Baseline |
| **Warm Cache Hit** | `symtrace express HEAD~1 HEAD` (Warm) | 2 | 6,011 | 3.77 ms | 1.24 ms | 22.21 ms | **3.91× faster** |
| **JSON Output** | `symtrace express HEAD~1 HEAD --json` | 2 | 6,011 | 3.66 ms | 1.00 ms | 18.38 ms | **4.72× faster** |
| **Full Test Suite** | `cargo test --all` (186 tests) | N/A | ~65,000 | N/A | N/A | **30.00 ms** (0.03s) | **186 tests / 30ms** |

## 16. Security & Supply Chain Matrix

- **Safe Rust Guarantee:** Enforced crate-wide via `#![deny(unsafe_code)]`.
- **Zero Network Access:** Operates 100% offline with zero remote network calls.
- **Resource Limits:** Hard limits on file size (5 MiB default), node count (200k default), recursion depth (2048), and parse timeouts (2s).
- **Process Isolation:** Pager executed directly via explicit argument vectors (no `sh -c` / `cmd.exe`).
- **Related Documents:** See [SECURITY.md](SECURITY.md), [BENCHMARKS.md](BENCHMARKS.md), and [CHANGELOG.md](CHANGELOG.md).
