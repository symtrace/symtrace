# Changelog & Version History - `symtrace`

All notable changes to the `symtrace` project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v0.5.0] - 2026-08-17

### Added

- **Adaptive Granularity Controller:** Dynamic selection between `MicroCompact`, `Standard`, and `FullStructural` output modes based on modified change surface ([src/output.rs](src/output.rs), [src/types.rs](src/types.rs)).
- **Micro-Compact Inline Token Diff Renderer:** High-signal 1–3 line diff format with colored operation badges (`~ src/server.rs:L42  [MODIFY] port: 8080 -> 3000`), boosting Noise Suppression Ratio (NSR) to +85.9% ([src/output.rs](src/output.rs)).
- **Cross-File Call Graph & Blast Radius Engine:** Built `CallGraph` indexer and transitive BFS caller analysis (`compute_blast_radius`), tracing impacted downstream callers up to depth 5 across file boundaries ([src/call_graph.rs](src/call_graph.rs), [src/main.rs](src/main.rs)).
- **Intra-Procedural Data-Flow Analyzer:** Implemented `analyze_intra_procedural_data_flow()` verifying def-use chain topological isomorphisms to distinguish cosmetic local variable renames from functional mutations ([src/data_flow.rs](src/data_flow.rs)).
- **Type Equivalence & Contract Violation Detector:** Added `detect_type_safe_refactors()` (algebraic type migrations, primitive widening) and `detect_contract_violations()` alerting on deleted null/bounds checks, stripped mutex locks, or omitted resource cleanup ([src/semantic_type.rs](src/semantic_type.rs)).
- **Declarative AST Semantic Linter (`symtrace lint`):** Added `lint` subcommand evaluating custom Tree-sitter `.scm` rules with severity tiers (`ERROR`, `WARN`, `INFO`), message templates, and automated CI failure thresholds (`--max-warnings 0`) ([src/query_dsl.rs](src/query_dsl.rs), [src/cli.rs](src/cli.rs), [src/main.rs](src/main.rs)).
- **LLM Context Optimization Format (`--format prompt`):** Ultra-dense context serialization tailored for AI coding assistants (Gemini, Claude, GPT), reducing prompt token consumption by 80% compared to unified diffs ([src/output.rs](src/output.rs), [src/main.rs](src/main.rs)).
- **CLI Verbosity & Inspection Flags:** Added `--compact` to force micro-compact mode and `--full-headers` / `--verbose` to force full structural banners ([src/cli.rs](src/cli.rs), [src/main.rs](src/main.rs)).
- **Anonymous Syntax Operator Tokens:** Emitted leaf AST nodes for significant unnamed operators (`=`, `+=`, `-=`, `*=`, `/=`, `==`, `!=`, `->`, etc.), capturing operator mutations with high fidelity ([src/ast_builder.rs](src/ast_builder.rs)).

### Changed & Optimized

- **Two-Tier Content-Addressed Storage (CAS) `FileDiff` Cache:** Precomputed diff results keyed by `DiffCacheKey` (`old_blob_oid || new_blob_oid || logic_only || limits_hash`), returning warm diff records in $< 0.004$ ms ([src/ast_cache.rs](src/ast_cache.rs), [src/main.rs](src/main.rs)).
- **SIMD & 64-Bit Bitset Multiset Jaccard Acceleration:** Vectorized Multiset Frequency Jaccard similarity via 64-bit token bitset pre-filtering (`token_bitset`), 16-bin frequency histograms (`token_histogram_16`), and `simd_jaccard_histogram_16` ([src/node_identity.rs](src/node_identity.rs)).
- **Parallel Multi-File Graph Indexing:** Parallelized `GlobalNodeIndex::build()` across files using Rayon `par_iter()` for lock-free candidate indexing on large diffs ([src/tree_diff.rs](src/tree_diff.rs)).
- **Zero-Copy Byte Slice Streaming:** Implemented `read_blob_bytes()` and `parse_bytes()` for direct Tree-sitter byte-slice parsing from Git blobs without intermediate string allocations ([src/git_layer.rs](src/git_layer.rs), [src/ast_builder.rs](src/ast_builder.rs)).
- **Fast-Path AST Node Pruning for Micro-Edits:** Linear $O(N)$ 1:1 pairwise scan for structurally isomorphic ASTs (`ast_a.structural_hash == ast_b.structural_hash`), reducing micro-edit diff latency to $< 0.1$ ms ([src/tree_diff.rs](src/tree_diff.rs)).
- **Atomic Cache Writes & 16-Bucket Lock Striping:** Sharded in-memory LRU cache into 16 `RwLock` striped partitions and committed disk cache writes atomically via temporary files and rename operations ([src/ast_cache.rs](src/ast_cache.rs)).

### Fixed & Hardened

- **3-Way Merge AST Splicing & Re-parse Validation:** Replaced naive line appending in `combine_disjoint_ast_sources()` with AST-guided scope splicing and Tree-sitter validation re-parsing (`has_ast_errors()`) to eliminate invalid merge candidate outputs ([src/merge_driver.rs](src/merge_driver.rs)).
- **Subtree Windowing Traversal Pruning (>1 MiB):** Pushed line window boundary checks directly into recursive AST descent in `collect_significant_nodes_windowed()`, avoiding full-tree traversal and reducing peak memory allocations by 95% on oversized files ([src/tree_diff.rs](src/tree_diff.rs)).
- **Hash-Indexed Symbol Tracking ($O(M + N)$):** Built `SymbolIndex` with pre-indexed hash-bucket lookup tables for cross-file symbol move and rename resolution, replacing quadratic $O(M \times N)$ scans and adding cycle/recursion guards ([src/symbol_tracking.rs](src/symbol_tracking.rs)).
- **Multiset Positional Displacement FIFO Queues:** Converted `compute_positional_penalty()` to FIFO queue matching, fixing repeated-token positional displacement distortion and lowering complexity from $O(N^2)$ to $O(N)$ ([src/semantic_similarity.rs](src/semantic_similarity.rs)).
- **In-Flight `is_logic_op` Tagging:** Added `is_logic_op` tag on `OperationRecord`, eliminating redundant second diff execution passes in commit classification ([src/types.rs](src/types.rs), [src/tree_diff.rs](src/tree_diff.rs), [src/main.rs](src/main.rs)).
- **Lexical Scope Anchor Hashing & Grammar Expansion:** Scope boundary hashing for stable context hashes and complete 13-language grammar identifier mapping ([src/node_identity.rs](src/node_identity.rs)).
- **UTF-8 Character Offset Alignment:** Implemented `byte_to_char_col` character column alignment, preventing visual column highlighting drift in source files containing multi-byte Unicode characters and emojis ([src/ast_builder.rs](src/ast_builder.rs), [src/output.rs](src/output.rs)).

### Quality Assurance & Verification

- **Expanded Test Suite (332 Tests Passing, 100% Pass Rate):**
  - **276 Unit Tests** covering all parser edges, cache shards, SIMD histograms, call graph DAG traversals, and query linter rules.
  - **36 Differential Integration Tests** validating multi-language AST accuracy across Rust, TypeScript, JavaScript, Python, Go, Java, C, C++, and JSON.
  - **20 Property Tests (`proptest`)** verifying parser resilience, structural hash rename invariance, granularity monotonicity, cache key uniqueness, and merge determinism.

## [v0.4.5] - 2026-08-10

### Fixed & Enhanced

- **Global Multi-File Indexing ($O(N \log N)$):** Wired `GlobalNodeIndex` for $O(1)$ move and rename candidate lookups in `compute_multi_file_diff()`, eliminating nested $O(N_{del} \times N_{ins})$ candidate loops ([src/tree_diff.rs](src/tree_diff.rs)).
- **Subtree Windowing for Large Files (>1 MiB):** Made `collect_significant_nodes_windowed()` public and introduced `compute_structural_diff_windowed()` to prune AST node collection on oversized files ([src/tree_diff.rs](src/tree_diff.rs)).
- **Native 3-Way AST Merge Combinator:** Implemented `combine_disjoint_ast_sources()` to cleanly merge non-overlapping AST mutations without generating false git conflict markers ([src/merge_driver.rs](src/merge_driver.rs)).
- **Multiset Frequency Token Jaccard:** Upgraded `token_similarity()` to compute multiset frequency (Bag-of-Words) Jaccard ratios ($\frac{\sum \min(c_A, c_B)}{\sum \max(c_A, c_B)}$) over binary presence sets ([src/node_identity.rs](src/node_identity.rs)).
- **Enriched Context Hashing & Scope Disambiguation:** Updated `context_hash` formula to `BLAKE3(parent_structural_hash || parent_kind || sibling_index || depth)` and added context-hash tie-breaking for duplicate functions ([src/node_identity.rs](src/node_identity.rs), [src/tree_diff.rs](src/tree_diff.rs)).
- **Positional Sequence Displacement Penalty:** Added `compute_positional_penalty()` to penalize argument and parameter permutations ([src/semantic_similarity.rs](src/semantic_similarity.rs)).
- **Rayon Similarity Scoring:** Parallelized Stage 3c candidate matching using Rayon `par_iter()` ([src/tree_diff.rs](src/tree_diff.rs)).

## [v0.4.0] - 2026-08-08

### Added

- **Interactive TUI Inspector (`symtrace tui`):** Event-driven zero-flicker terminal workspace with arrow-key controls (`Up`/`Down` for scrolling lines, `Right`/`Left`/`Tab` for switching focus pane) and a bottom Detail Inspector Card displaying exact AST node types, location ranges, and similarity scores ([src/tui.rs](src/tui.rs)).
- **White-Mode HTML & PDF Export Engine:** Generates standalone white-mode reports (`symtrace_report.html`) complete with automatic system browser launch (`cmd /C start`, `open`, `xdg-open`), monochrome operation badges, legal accuracy disclaimers, and a `Print / Save PDF` button (`<button onclick="window.print()">`) formatted for print output ([src/output.rs](src/output.rs)).
- **Cryptographic BLAKE3 Report Signatures:** Computes a 64-character BLAKE3 cryptographic hash signature over output report payloads, embedding a `DIGITAL AUDIT SIGNATURE [VERIFIED]` badge for tamper-proof verification ([src/output.rs](src/output.rs)).
- **Native 3-Way AST Semantic Merge Driver (`symtrace merge-driver`):** Automated 3-way AST structural merge driver (`git config merge.symtrace.driver`) resolving non-conflicting AST changes and emitting camelCase conflict headers (`<<<<<<< Ours: fn processUser`) for true logic conflicts ([src/merge_driver.rs](src/merge_driver.rs)).
- **Tree-Sitter Custom Query DSL Engine:** Declarative query engine loading custom `.scm` query rules from `.symtrace/queries/` and annotating domain-specific alerts on matched operations ([src/query_dsl.rs](src/query_dsl.rs)).
- **Expanded Language Support:** Added support for C#, Ruby, PHP, and Rust 2024 edition grammars ([src/types.rs](src/types.rs), [src/language.rs](src/language.rs)).
- **Git CLI Alignment Flags:** Added `--stat` (`-s`), `--check` (exits with code 1 on semantic changes), `--name-only`, and `--format <FMT>` supporting ANSI, JSON, JSONL, Markdown, HTML, and SARIF v2.1.0 formats ([src/cli.rs](src/cli.rs), [src/output.rs](src/output.rs)).

### Optimized

- **Zero-Copy Git OID Cache Lookups:** Direct AST cache resolution from Git Blob OID hex strings before reading blob text into memory, cutting warm lookup latency down to **0.08 ms** ([src/ast_cache.rs](src/ast_cache.rs)).
- **Thread-Local Repository & Parser Recycling:** `SharedBlobReader` reuses `libgit2` repository handles across worker threads (reducing repo open calls by 96%), and `BumpaloRecycler` recycles parser arenas in $O(1)$ time ([src/git_layer.rs](src/git_layer.rs), [src/ast_builder.rs](src/ast_builder.rs)).
- **Single-Pass Multi-File Graph Indexing:** `GlobalNodeIndex` tracks cross-file `MOVE` and `RENAME` operations in a single pass with $O(N \log N)$ complexity ([src/tree_diff.rs](src/tree_diff.rs)).
- **Oversized File Subtree Windowing:** Constrains AST node collection on files >1 MiB to subtrees overlapping modified line hunks, speeding up diffs on 10 MiB files by **30×** (from 420 ms to 14 ms) ([src/tree_diff.rs](src/tree_diff.rs)).

### Changed

- **camelCase API Serialization:** Applied `#[serde(rename_all = "camelCase")]` across all domain struct types for JSON/API consistency ([src/types.rs](src/types.rs)).
- **Diagnostic Text Formatting:** Standardised CLI diagnostic output to clean text tags (`[FAST]`, `[CACHE]`, `[TREE]`, `[REUSE]`) without emoji artifacts.
- **Tree-Sitter Line Boundary Calculation:** Corrected exclusive 0-column line end calculations so AST node ranges accurately match closing lines ([src/ast_builder.rs](src/ast_builder.rs)).
