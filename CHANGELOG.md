# Changelog & Version History - `symtrace`

All notable changes to the `symtrace` project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
