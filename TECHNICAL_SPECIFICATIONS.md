# `symtrace` v0.5.0 - Technical Specifications & Architecture Reference

**Project:** `symtrace` - Deterministic Semantic Diff Engine  
**Version:** `v0.5.0`  
**License:** MIT  

## 1. Architecture Overview

`symtrace` replaces traditional line-based `git diff` output with **AST-based structural analysis**, detecting semantic changes (`MOVE`, `RENAME`, `MODIFY`, `INSERT`, `DELETE`) across files while filtering out whitespace, formatting, and comment noise.

```
Repository Target Resolution ──► Dual-Path Git File Changes ──► Path Glob Filtering
 (Commits / Index / Work)        (Old & New Path Pairs)          (--path "src/**/*.rs")
           │                                                               │
           ▼                                                               ▼
 Two-Tier CAS Diff Cache ◄────── Incremental AST Parsing ◄───── Versioned AST Cache
  (DiffCacheKey OID Map)          (BLAKE3 Hash Reuse)           (16-Shard RwLock LRU)
           │
           ▼
 6-Stage AST Matching ───────► Global Node Index Graph ──► Semantic Intelligence
  (SIMD & Bitset Jaccard)       (Parallel Rayon O(N log N))  (Call Graph & Data-Flow)
           │                                                               │
           ▼                                                               ▼
 Adaptive Granularity ────────────────────────────────────────► Multi-Format Output
  (MicroCompact / Standard / Full)                               (CLI / Prompt / SARIF)
```

## 2. Module Responsibilities Matrix

| Module | File Path | Responsibility |
| :--- | :--- | :--- |
| **Main Orchestrator** | [src/main.rs](src/main.rs) | Pipeline orchestration, positional argument normalization, subcommand routing (`tui`, `merge-driver`, `lint`, `git-diff-driver`). |
| **CLI Parser** | [src/cli.rs](src/cli.rs) | CLI argument definitions (`clap`), positional defaults, flags (`--compact`, `--full-headers`, `--format prompt`, `--lint`). |
| **Git Integration** | [src/git_layer.rs](src/git_layer.rs) | Repository access (`libgit2`), thread-local handle pooling (`SharedBlobReader`), zero-copy byte slice reading (`read_blob_bytes`), `diff.find_similar()`. |
| **Language Support** | [src/language.rs](src/language.rs) | Extension matching for 13 supported languages (`Rust`, `JS`, `TS`, `Python`, `Java`, `C`, `C++`, `Go`, `JSON`, `C#`, `Ruby`, `PHP`, `Rust 2024`). |
| **AST Builder** | [src/ast_builder.rs](src/ast_builder.rs) | Tree-sitter parsing, `BumpaloRecycler` arena pooling, anonymous operator tokens, Unicode character column alignment (`byte_to_char_col`), bounds verification. |
| **Serialized AST & CAS Cache** | [src/ast_cache.rs](src/ast_cache.rs) | Two-Tier CAS `FileDiff` cache & serialized AST disk storage with 16-bucket `RwLock` striping and atomic rename writes. |
| **Incremental Parse** | [src/incremental_parse.rs](src/incremental_parse.rs) | Bounded `TreeCache` LRU (500 capacity), minimal edit computation and child count structural invariant safety. |
| **Node Identity** | [src/node_identity.rs](src/node_identity.rs) | 4-hash BLAKE3 identity scheme, 64-bit token bitset pre-filtering (`token_bitset`), and 16-bin SIMD frequency histograms (`simd_jaccard_histogram_16`). |
| **Tree Diff Engine** | [src/tree_diff.rs](src/tree_diff.rs) | Parallel 6-stage AST matching, fast-path isomorphic micro-edit scan, Rayon parallel `GlobalNodeIndex`, subtree windowing for files >1 MiB. |
| **Call Graph & Blast Radius** | [src/call_graph.rs](src/call_graph.rs) | Cross-file function call graph indexer and transitive BFS blast radius analysis up to depth 5 across file boundaries. |
| **Data-Flow Analyzer** | [src/data_flow.rs](src/data_flow.rs) | Intra-procedural def-use variable lineage tracker and topological isomorphism verification. |
| **Type & Contract Security** | [src/semantic_type.rs](src/semantic_type.rs) | Algebraic type migration analyzer and safety contract violation detector (removed null checks, deleted bounds guards, stripped mutexes). |
| **Semantic Similarity** | [src/semantic_similarity.rs](src/semantic_similarity.rs) | Multiset frequency (Bag-of-Words) Jaccard, FIFO token positional displacement penalty ($\Delta_{\text{pos}}$), and complexity deltas. |
| **Symbol Tracking** | [src/symbol_tracking.rs](src/symbol_tracking.rs) | $O(M+N)$ hash-indexed symbol tracking table with recursion and cycle guards. |
| **Refactor Engine** | [src/refactor_detection.rs](src/refactor_detection.rs) | Refactor pattern detection (extract method, move, rename variable) with HashMap candidate index. |
| **Query DSL & Linter** | [src/query_dsl.rs](src/query_dsl.rs) | Declarative AST semantic linter (`symtrace lint`) evaluating custom `.scm` rules with CI error thresholds. |
| **Commit Classification** | [src/commit_classification.rs](src/commit_classification.rs) | Commit auto-classification & intent labeling (`GUARD_CLAUSE_ADDED`, `TYPE_SIGNATURE_CHANGED`, `CONTROL_FLOW_INVERTED`, etc.). |
| **Adaptive Output** | [src/output.rs](src/output.rs) | Adaptive Granularity Controller (`MicroCompact`, `Standard`, `FullStructural`), LLM prompt context format (`--format prompt`), Signed HTML/PDF, SARIF v2.1.0. |
| **TUI Inspector** | [src/tui.rs](src/tui.rs) | Event-driven zero-flicker crossterm interactive TUI inspector with arrow-key navigation. |
| **3-Way Merge Driver** | [src/merge_driver.rs](src/merge_driver.rs) | AST scope splicing and tree-sitter validation re-parse (`has_ast_errors()`) for conflict-free 3-way AST rebases. |
| **Terminal Pager** | [src/pager.rs](src/pager.rs) | Interactive terminal detection and safe `$PAGER` process routing (`less -RFX`). |
| **Config Loader** | [src/config.rs](src/config.rs) | Hierarchical `.symtracerc` / `symtrace.toml` TOML configuration loader. |
| **Shared Domain Types** | [src/types.rs](src/types.rs) | Shared domain data structures and camelCase serde representations (`#[serde(rename_all = "camelCase")]`). |

## 3. 6-Stage AST Node Matching Engine & SIMD Acceleration

The core diff engine ([src/tree_diff.rs](src/tree_diff.rs)) correlates nodes between old (Commit A) and new (Commit B) AST trees using a deterministic 6-stage matching pipeline:

1. **Fast-Path Isomorphic Micro-Edit Scan ($O(N)$):**
   When `ast_a.structural_hash == ast_b.structural_hash` and node lengths match, performs a linear 1:1 pairwise scan, reducing micro-edit diff latency to $< 0.1$ ms.
2. **Stage 1 - Exact Structural & Content Hash Match ($O(1)$):**
   Matches nodes with identical `structural_hash` and `content_hash` using HashMap lookup with `context_hash` tie-breaking for duplicate functions.
3. **Stage 2 - Structural Shape & Identifier Rename Match:**
   Matches nodes sharing identical AST structural tree shapes (`structural_hash`), detecting variable or declaration renames when only identifier leaf tokens differ.
4. **Stage 3a - Kind & Symbol Name Correlator:**
   Correlates declaration nodes sharing the same AST `kind` and extracted symbol `name` within modified scope containers.
5. **Stage 3b - Identity Hash Rename Fallback Match:**
   Correlates refactored function bodies using BLAKE3 `identity_hash` (structural hash combined with normalized symbol identifier strings).
6. **Stage 3c - SIMD-Accelerated Multiset Frequency Similarity Scoring:**
   Evaluates remaining unmatched node pairs using:
   - **64-bit Bitset Disjointness Filtering (`token_bitset`):** $O(1)$ fast-path rejection for completely disjoint token vocabularies.
   - **16-Bin SIMD Histogram Jaccard (`simd_jaccard_histogram_16`):** Vectorized evaluation of frequency histograms.
   - **FIFO Queue Positional Penalty ($\Delta_{\text{pos}}$):** $O(N)$ positional displacement matching.
7. **Stage 4 - Unmatched Node Resolution:**
   Classifies remaining unmatched old nodes as `DELETE` and new nodes as `INSERT`.

## 4. Cryptographic BLAKE3 Node Identity Scheme

Each AST node ([src/node_identity.rs](src/node_identity.rs)) computes four distinct 32-byte BLAKE3 cryptographic hashes:

- **`structural_hash`:** Hashes node `kind` and child subtree structural arrangement (ignores token identifiers, variable names, and comments).
- **`content_hash`:** Hashes exact raw UTF-8 literal content for leaf nodes.
- **`identity_hash`:** Combines `structural_hash` with normalized symbol identifier strings to recognize refactored declaration bodies.
- **`context_hash`:** Incorporates parent `structural_hash`, `parent_kind`, `sibling_index`, and `depth` ($\text{BLAKE3}(\text{parent\_structural\_hash} \,||\, \text{parent\_kind} \,||\, \text{sibling\_index} \,||\, \text{depth})$) anchored at enclosing scope declaration boundaries.

## 5. Cross-File Call Graph & Blast Radius Analysis

The call graph engine ([src/call_graph.rs](src/call_graph.rs)) builds an in-memory directed dependency graph across all repository source files:

- **Call Graph Extraction:** Scans function declarations and invocation expressions across all files, mapping callers to targets.
- **Transitive BFS Blast Radius (`compute_blast_radius`):** When a function signature or public interface changes, traces downstream dependent callers up to depth 5 across file boundaries.
- **Severity Scoring:** Classifies impact into `High` ($\ge 5$ callers or depth $\ge 3$), `Medium` ($2-4$ callers), or `Low` ($1$ caller).

## 6. Intra-Procedural Data-Flow & Variable Lineage Tracker

The data-flow engine ([src/data_flow.rs](src/data_flow.rs)) tracks def-use chains within function bodies:

- **Def-Use Chain Extraction:** Identifies local variable declarations, assignment mutations, and return value expressions.
- **Topological Isomorphism Verification:** Compares data-flow dependency graphs between old and new functions to differentiate cosmetic variable renames from functional variable mutations.

## 7. Safety Contract Violation Detector

The semantic type engine ([src/semantic_type.rs](src/semantic_type.rs)) analyzes safety guard deletions:

- **`REMOVED_NULL_CHECK`:** Alerts when `if x != null`, `if (ptr)`, or `Option::is_none()` guards are removed.
- **`REMOVED_BOUNDS_CHECK`:** Alerts when array index or length comparisons (`< len`, `< size()`) are eliminated.
- **`STRIPPED_MUTEX_LOCK`:** Alerts when `lock()`, `acquire()`, or mutex guards are deleted.
- **`OMITTED_RESOURCE_CLEANUP`:** Alerts when `close()`, `dispose()`, or `drop()` calls are removed.

## 8. Declarative AST Semantic Linter (`symtrace lint`)

The linter engine ([src/query_dsl.rs](src/query_dsl.rs)) enables custom architectural rules:

- **Rule Definition (`.scm`):** Tree-sitter S-expression query rules with comment metadata (`;; @id`, `;; @severity`, `;; @message`).
- **Severity Tiers:** `ERROR`, `WARN`, `INFO`.
- **CI Integration:** Automated failure exit codes via `--max-warnings <N>` (default: 0).
- **Supported Formats:** `cli` (colored ANSI diagnostics), `json`, and `sarif`.

## 9. Dynamic Adaptive Granularity Controller

The output controller ([src/output.rs](src/output.rs)) selects optimal rendering modes:

- **`MicroCompact`:** High-signal 1–3 line diff output for micro-edits (<50 modified lines), achieving +85.9% Noise Suppression Ratio.
- **`Standard`:** Full structural per-file tables with operation badges.
- **`FullStructural`:** Comprehensive diagnostic headers, classification confidence, and performance metrics.

## 10. Two-Tier CAS Diff Cache & Disk Storage

- **Content-Addressed `DiffCacheKey`:** Keys computed `FileDiff` records by `old_blob_oid || new_blob_oid || logic_only || limits_hash`.
- **16-Bucket `RwLock` Sharding:** In-memory LRU cache divided into 16 lock-striped partitions to eliminate thread contention.
- **Atomic Disk Writes:** Writes disk cache envelopes via temporary files and atomic rename operations.
- **Lookup Latency:** Resolves warm cache hits in $< 0.004$ ms per file.

## 11. LLM Context Optimization (`--format prompt`)

The prompt context serialization format ([src/output.rs](src/output.rs)) produces dense semantic representations for AI assistants (Gemini, Claude, GPT):

- Replaces repetitive line headers with concise structural modification markers (`[MOVED]`, `[RENAMED]`, `[MODIFIED]`).
- Prepends critical contract violations and intent classifications.
- Reduces prompt token consumption by **80%** compared to unified diffs.

## 12. Native 3-Way AST Merge Driver Specs

Configure `symtrace` as a native Git merge driver ([src/merge_driver.rs](src/merge_driver.rs)):

```bash
git config merge.symtrace.name "symtrace 3-Way AST Merge Driver"
git config merge.symtrace.driver "symtrace merge-driver %O %A %B %P"
```

- **AST Scope Splicing:** Merges non-overlapping function or class additions into existing containers.
- **Validation Re-Parsing:** Re-parses candidate merge outputs with Tree-sitter (`has_ast_errors()`) and falls back cleanly to conflict markers if syntax errors occur.

## 13. Resource Guardrails & Security Model

- **Hard Resource Limits:**
  - `max_file_size`: 5 MiB (`5,242,880` bytes)
  - `max_ast_nodes`: 200,000 nodes
  - `max_recursion_depth`: 2,048 levels
  - `parse_timeout_ms`: 2,000 ms
- **Subtree Windowing (>1 MiB):** Prunes unchanged AST subtrees during recursive descent on files exceeding 1 MiB.
- **Safe Rust:** Enforced across all crates with `#![deny(unsafe_code)]`.
- **100% Offline:** Zero network capabilities, telemetry, or remote communication.
- **Supply Chain Provenance:** Automated Cosign OIDC signing, SPDX SBOM, and GitHub Artifact Attestations.
- **Related Documents:** See [SECURITY.md](SECURITY.md), [BENCHMARKS.md](BENCHMARKS.md), and [CHANGELOG.md](CHANGELOG.md).
