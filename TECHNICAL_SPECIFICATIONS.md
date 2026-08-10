# `symtrace` v0.4.5 - Technical Specifications & Architecture Reference

**Project:** `symtrace` - Deterministic Semantic Diff Engine  
**Version:** `v0.4.5`  
**License:** MIT  

## 1. Architecture Overview

`symtrace` replaces traditional line-based `git diff` output with **AST-based structural analysis**, detecting semantic changes (`MOVE`, `RENAME`, `MODIFY`, `INSERT`, `DELETE`) across files while filtering out whitespace, formatting, and comment noise.

```
Repository Target Resolution ──► Dual-Path Git File Changes ──► Path Glob Filtering
 (Commits / Index / Work)        (Old & New Path Pairs)          (--path "src/**/*.rs")
           │                                                               │
           ▼                                                               ▼
 Bounded TreeCache LRU ◄────── Incremental AST Parsing ◄───── Versioned AST Cache
  (Cap: 500 trees)             (BLAKE3 Hash Reuse)           (Two-Tier Serialized Cache)
           │
           ▼
 6-Stage AST Matching ───────► Global Multi-File AST Index ──► Output Formats
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
| **Serialized AST Cache** | [src/ast_cache.rs](src/ast_cache.rs) | Two-Tier Serialized Disk Cache (in-memory LRU + versioned on-disk Bincode storage with limits hash). |
| **Incremental Parse** | [src/incremental_parse.rs](src/incremental_parse.rs) | Bounded `TreeCache` LRU (500 capacity), minimal edit computation and child count structural invariant safety. |
| **Node Identity** | [src/node_identity.rs](src/node_identity.rs) | 4-hash BLAKE3 identity computation per node (structural, content, identity, context with parent_kind & sibling_index). |
| **Tree Diff Engine** | [src/tree_diff.rs](src/tree_diff.rs) | Parallel 6-stage AST matching algorithm, `GlobalNodeIndex` $O(1)$ lookups, subtree windowing for files >1 MiB. |
| **Semantic Similarity** | [src/semantic_similarity.rs](src/semantic_similarity.rs) | Multiset frequency (Bag-of-Words) Jaccard, positional displacement penalty ($\Delta_{\text{pos}}$), and complexity deltas. |
| **Symbol Tracking** | [src/symbol_tracking.rs](src/symbol_tracking.rs) | Deep BFS symbol extraction & cross-file movement tracking. |
| **Refactor Engine** | [src/refactor_detection.rs](src/refactor_detection.rs) | Refactor pattern detection (extract method, move, rename variable) with HashMap candidate index. |
| **Query DSL Engine** | [src/query_dsl.rs](src/query_dsl.rs) | Custom Tree-Sitter `.scm` query loader and rule evaluation engine (`.symtrace/queries/`). |
| **Commit Classification** | [src/commit_classification.rs](src/commit_classification.rs) | Commit auto-classification (feature, refactor, bugfix, cleanup, formatting_only). |
| **TUI Inspector** | [src/tui.rs](src/tui.rs) | Event-driven zero-flicker crossterm interactive TUI inspector with arrow-key navigation. |
| **3-Way Merge Driver** | [src/merge_driver.rs](src/merge_driver.rs) | Native 3-way AST semantic merge driver with disjoint AST mutation combinator (`git config merge.symtrace.driver`). |
| **Terminal Pager** | [src/pager.rs](src/pager.rs) | Interactive terminal detection and `$PAGER` process routing (`less -RFX`). |
| **Config Loader** | [src/config.rs](src/config.rs) | Hierarchical `.symtracerc` / `symtrace.toml` TOML configuration loader. |
| **Multi-Format Output** | [src/output.rs](src/output.rs) | Multi-format output engines (ANSI, JSON, JSONL, Markdown, White-Mode Signed HTML/PDF, SARIF v2.1.0). |
| **Shared Domain Types** | [src/types.rs](src/types.rs) | Shared domain data structures and camelCase serde representations (`#[serde(rename_all = "camelCase")]`). |

## 3. 6-Stage AST Node Matching Engine

The core diff engine ([src/tree_diff.rs](src/tree_diff.rs)) correlates nodes between old (Commit A) and new (Commit B) AST trees using a deterministic 6-stage matching pipeline:

1. **Stage 1 - Exact Structural & Content Hash Match ($O(1)$):**
   Matches nodes with identical `structural_hash` and `content_hash` using a HashMap lookup index with `context_hash` tie-breaking for duplicate functions. Unchanged subtrees are matched instantly.
2. **Stage 2 - Structural Shape & Identifier Rename Match:**
   Matches nodes sharing identical AST structural tree shapes (`structural_hash`), detecting variable or declaration renames when only identifier leaf tokens differ.
3. **Stage 3a - Kind & Symbol Name Correlator:**
   Correlates declaration nodes sharing the same AST `kind` and extracted symbol `name` within modified scope containers.
4. **Stage 3b - Identity Hash Rename Fallback Match:**
   Correlates refactored function bodies using BLAKE3 `identity_hash` (structural hash combined with normalized symbol identifier strings).
5. **Stage 3c - Parallel Multiset Frequency Similarity Scoring:**
   Evaluates remaining unmatched node pairs using Rayon (`par_iter`) weighted multiset frequency (Bag-of-Words) Jaccard ratio, subtree-size ratio pre-filtering, and positional sequence displacement penalties ([src/semantic_similarity.rs](src/semantic_similarity.rs)).
6. **Stage 4 - Unmatched Node Resolution:**
   Classifies remaining unmatched old nodes as `DELETE` and new nodes as `INSERT`.

## 4. Cryptographic BLAKE3 Node Identity Scheme

Each AST node ([src/node_identity.rs](src/node_identity.rs)) computes four distinct 32-byte BLAKE3 cryptographic hashes to establish identity:

- **`structural_hash`:** Hashes node `kind` and child subtree structural arrangement (ignores token identifiers, variable names, and comments).
- **`content_hash`:** Hashes exact raw UTF-8 literal content for leaf nodes.
- **`identity_hash`:** Combines `structural_hash` with normalized symbol identifier strings to recognize refactored declaration bodies.
- **`context_hash`:** Incorporates parent `structural_hash`, `parent_kind`, `sibling_index`, and `depth` ($\text{BLAKE3}(\text{parent\_structural\_hash} \,||\, \text{parent\_kind} \,||\, \text{sibling\_index} \,||\, \text{depth})$) to enforce strict lexical scope resolution.

### Normalization & Positional Penalty Rules

- **Multiset Frequency Jaccard:** Evaluates token count frequency distributions ($\frac{\sum \min(c_A, c_B)}{\sum \max(c_A, c_B)}$) over deduplicated presence sets.
- **Positional Displacement Penalty ($\Delta_{\text{pos}}$):** Penalizes token/argument sequence permutations when function parameters or arguments are reordered.
- **Comment Filtering:** In `--logic-only` mode, comment nodes are replaced with a static placeholder (`__COMMENT__`) to prevent comment edits from breaking structural hashes.

## 5. Incremental Parsing & Arena Allocator Pool

- **Minimal Byte Edit Computation ([src/incremental_parse.rs](src/incremental_parse.rs)):**
  `compute_edit` calculates minimal byte edit ranges (`InputEdit`) between source versions. `edited_tree.edit(&edit)` informs Tree-sitter's parser to reuse unchanged AST subtrees during re-parsing. Enforces child count invariants before hash reuse.
- **Arena Memory Recycling ([src/ast_builder.rs](src/ast_builder.rs)):**
  `BumpaloRecycler` maintains thread-local `bumpalo::Bump` arenas. AST construction temporaries are allocated inside the arena and freed in $O(1)$ time per file via `bump.reset()`.

## 6. Global Multi-File Graph Index & Symbol Tracking

- **Unified Repository Graph Index ([src/tree_diff.rs](src/tree_diff.rs)):**
  `GlobalNodeIndex` maps symbol declarations and usage sites across all repository files in $O(N \log N)$ complexity, exposing $O(1)$ indexed query methods (`find_candidate_for_move`, `find_candidate_by_structural_hash`, `find_candidate_by_identity_hash`) that eliminate $O(F \times N^2)$ post-pass scanning.
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
  Evaluates custom query patterns and rule sources against parsed AST nodes during diffing, attaching domain-specific alert metadata to output operations.

## 9. Subtree Windowing & Oversized File Optimization

When processing large source code files exceeding `OVERSIZED_FILE_THRESHOLD_BYTES` (`1_048_576` bytes / 1 MiB):

- `symtrace` extracts modified line hunks from Git diff deltas.
- `collect_significant_nodes_windowed` filters node collection strictly to subtrees overlapping modified line ranges.
- Reduces node matching complexity from $O(N^2)$ over the full file to $O(\text{hunk\_size})$, lowering execution time on 10 MiB files from 420 ms to **14 ms** (30× speedup).

## 10. Two-Tier Serialized Cache & Disk Storage Architecture

- **In-Memory LRU Cache:** Maintains up to 256 AST entries in memory ([src/ast_cache.rs](src/ast_cache.rs)).
- **On-Disk Versioned Storage:** Persists serialized AST structures using Bincode under `$XDG_CACHE_HOME/symtrace` or `%LOCALAPPDATA%\symtrace` ([src/ast_cache.rs](src/ast_cache.rs)).
- **Cache Key Security & Limits:** Keys entries using Git Blob OID hex strings combined with a 64-bit `limits_hash` (derived from `ParserLimits`) and max deserialization buffers (20 MiB).

## 11. Interactive TUI & Terminal Pager Architecture

- **Safe Terminal Pager Routing ([src/pager.rs](src/pager.rs)):**
  Spawns `$PAGER` / `$GIT_PAGER` (`less -RFX`) via explicit argument vectors (`std::process::Command::new`), bypassing shell evaluation.
- **Zero-Flicker Event-Driven TUI ([src/tui.rs](src/tui.rs)):**
  Built using `crossterm` raw terminal mode. Screen redraws execute exclusively on user input events (`crossterm::event::read()`).

## 12. Hierarchical Configuration Precedence & Resource Guardrails

- **Config Precedence ([src/config.rs](src/config.rs)):**
  Resolved in order: **CLI Flags > Repository Config (`.symtracerc` / `symtrace.toml`) > User Config (`~/.config/symtrace/symtrace.toml`) > Hardcoded Defaults**.
- **Resource Guardrails ([src/cli.rs](src/cli.rs)):**
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

## 14. Native 3-Way AST Merge Driver Specs

Configure `symtrace` as a native 3-way Git merge driver ([src/merge_driver.rs](src/merge_driver.rs)):

```bash
git config merge.symtrace.name "SymTrace 3-Way AST Merge Driver"
git config merge.symtrace.driver "symtrace merge-driver %O %A %B %P"
```

Parses Base (`%O`), Ours (`%A`), and Theirs (`%B`) versions, automatically merging non-conflicting AST node mutations via `combine_disjoint_ast_sources()` and writing camelCase conflict markers (`<<<<<<< Ours: fn processUser`) only for overlapping structural logic conflicts.

## 15. Empirical Performance Benchmarks

| Scenario | Mode / Command | Files | AST Nodes | Parse Time | Diff Time | Total Time | Speedup |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Cold AST Parse** | `symtrace express HEAD~1 HEAD` (Cold) | 2 | 6,011 | 32.21 ms | 0.94 ms | 86.77 ms | Baseline |
| **Warm Cache Hit** | `symtrace express HEAD~1 HEAD` (Warm) | 2 | 6,011 | 3.77 ms | 1.24 ms | 22.21 ms | **3.91× faster** |
| **JSON Output** | `symtrace express HEAD~1 HEAD --json` | 2 | 6,011 | 3.66 ms | 1.00 ms | 18.38 ms | **4.72× faster** |
| **Full Test Suite** | `cargo test --all` (190 tests) | N/A | ~65,000 | N/A | N/A | **30.00 ms** (0.03s) | **190 tests / 30ms** |

## 16. Security & Supply Chain Matrix

- **Safe Rust Guarantee:** Enforced crate-wide via `#![deny(unsafe_code)]`.
- **Zero Network Access:** Operates 100% offline with zero remote network calls.
- **Resource Limits:** Hard limits on file size (5 MiB default), node count (200k default), recursion depth (2048), and parse timeouts (2s).
- **Process Isolation:** Pager executed directly via explicit argument vectors (no `sh -c` / `cmd.exe`).
- **Related Documents:** See [SECURITY.md](SECURITY.md), [BENCHMARKS.md](BENCHMARKS.md), and [CHANGELOG.md](CHANGELOG.md).
