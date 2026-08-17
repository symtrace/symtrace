# Empirical Benchmarking Protocol & Research Plan for `symtrace`

**Document Title:** Robust Structural Benchmarking Scheme for `symtrace`, `git diff`, and Open-Source AST Diff Tools  
**Target Tool:** `symtrace` (v0.5.0)  
**Date:** August 2026  
**Status:** Peer-Review Ready Benchmark Specification  

## Executive Summary

Software diffing is fundamental to version control, code review, program analysis, and automated program repair. Traditional line-based diff tools (e.g., `git diff` using Myers, Histogram, or Patience algorithms) treat source code as flat text streams. This line-based paradigm produces high diff noise during formatting, comment, or whitespace changes, and fails to recognize structural refactorings such as function relocation (`MOVE`) or symbol renaming (`RENAME`).

`symtrace` is a deterministic, AST-based semantic diff engine written in Rust that operates directly on Abstract Syntax Trees via Tree-sitter parsers, utilizing a multi-stage AST node matching engine, BLAKE3 node identity hashing, thread-local arena recycling (`BumpaloRecycler`), SIMD-accelerated 16-bin multiset Jaccard comparison, Two-Tier CAS diff caching, and a single-pass global graph index (`GlobalNodeIndex`).

This document defines a **comprehensive, research-grade empirical benchmarking protocol** designed to evaluate `symtrace` against state-of-the-art line-based and structural diff tools (`git diff`, `git-delta`, `difftastic`, `GumTree`, `GitHub Semantic`) across large-scale open-source repositories and verified refactor datasets.

## 1. Research Objectives & Hypotheses

The benchmarking framework addresses eight primary Research Questions (RQs):

```
                       ┌─────────────────────────────────────────────────────────┐
                       │                Research Objectives (RQs)                │
                       └────────────────────────────┬────────────────────────────┘
                                                    │
         ┌───────────┬───────────┬───────────┬──────┴────┬───────────┬───────────┬───────────┬
         ▼           ▼           ▼           ▼           ▼           ▼           ▼           ▼
    ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐
    │   RQ1   │ │   RQ2   │ │   RQ3   │ │   RQ4   │ │   RQ5   │ │   RQ6   │ │   RQ7   │ │   RQ8   │
    │  Noise  │ │ Refactor│ │ CAS/SIMD│ │  Merge  │ │ Micro   │ │ LLM     │ │ Blast   │ │ Security│
    │ Filter  │ │ Track   │ │ Latency │ │ Driver  │ │ Granular│ │ Density │ │ Radius  │ │ & SARIF │
    └─────────┘ └─────────┘ └─────────┘ └─────────┘ └─────────┘ └─────────┘ └─────────┘ └─────────┘
```

### RQ1: Noise Suppression & Noise-to-Signal Ratio

* **Hypothesis $H_1$:** `symtrace` reduces diff noise on format-only, comment-only, and import-reordering commits by $\ge 95\%$ compared to line-based `git diff` engines, yielding zero false-positive semantic operations in `--logic-only` mode.

### RQ2: Refactoring Operation Classification Accuracy

* **Hypothesis $H_2$:** `symtrace` achieves significantly higher $F_1$-scores ($>0.92$) than `difftastic` and `GumTree` in detecting cross-file `MOVE` and `RENAME` operations on real-world multi-file commits, due to its single-pass $O(N \log N)$ `GlobalNodeIndex`.

### RQ3: Computational Performance & CAS Caching Efficiency

* **Hypothesis $H_3$:** Through Two-Tier Content-Addressed Storage (CAS) diff caching and 16-bin SIMD histogram bounding, `symtrace` returns warm diff records in $<0.004$ ms and outperforms `difftastic` by $10\times$ to $200\times$ on large multi-file diffs.

### RQ4: Native 3-Way AST Merge Conflict Reduction

* **Hypothesis $H_4$:** The `symtrace merge-driver` eliminates $\ge 80\%$ of text-based merge conflicts caused by concurrent non-overlapping refactorings compared to Git's default `ort` driver.

### RQ5: Adaptive Granularity on Micro-Commits

* **Hypothesis $H_5$:** The Adaptive Granularity Controller (`--compact`) compresses 1–3 line micro-edits into minimal 3-line structural summaries, converting negative noise suppression into $+76.9\%$ to $+85.9\%$ Noise Suppression Ratio ($NSR$).

### RQ6: AI / LLM Context Token Density (`--format prompt`)

* **Hypothesis $H_6$:** Dense semantic context serialization reduces LLM prompt token consumption by $\ge 80\%$ compared to unified diffs while surfacing critical security contract violations.

### RQ7: Cross-File Transitive Blast Radius Analysis

* **Hypothesis $H_7$:** Transitive BFS call graph indexing identifies $100\%$ of impacted downstream callers up to depth 5 across file boundaries.

### RQ8: Downstream Program Analysis & SARIF v2.1.0 Integration

* **Hypothesis $H_8$:** Structurally mapped SARIF v2.1.0 output integrates seamlessly with GitHub Code Scanning without line-drift false positives.

## 2. Benchmark Tool Suite & Baseline Taxonomy

| Tool Category | Tool Name | Parsing Model | Language Scope | Underlying Algorithm / Engine |
| :--- | :--- | :--- | :--- | :--- |
| **Line-Based Baselines** | `git diff (Myers)` | Flat Line Tokens | Language-Agnostic | LCS / Myers $O(ND)$ Algorithm |
| | `git diff --histogram` | Flat Line Tokens | Language-Agnostic | Low-frequency line matching |
| | `git diff --patience` | Flat Line Tokens | Language-Agnostic | Unique line sequence matching |
| | `git-delta` | Line + Syntax Highlight | Language-Agnostic | Line-based diff with terminal styling |
| **AST / Structural Tools** | `symtrace` (v0.5.0) | Concrete & Abstract AST | 13 Languages | SIMD / CAS 6-Stage AST Graph Engine |
| | `difftastic` | Structural / Tree-sitter | 30+ Languages | Dijkstra shortest-path on AST graph |
| | `GumTree` (v3.0) | Fine-Grained AST | Java, C, JS, Python | Top-down / Bottom-up AST Subtree Match |
| | `GitHub Semantic` | AST / Haskell | TS, Python, Go, Ruby | Tree-sitter AST diffing engine |
| **Merge Drivers** | `git merge (ort)` | Line-Based 3-Way | Language-Agnostic | Ostensibly Recursive's Twin driver |
| | `symtrace merge-driver` | 3-Way AST Driver | 13 Languages | AST Scope Splicing & Re-parse Validation |

## 3. Representative Open-Source Repository Corpus

The benchmarking corpus spans **13 programming languages** supported by `symtrace`, targeting top-tier open-source projects:

| Language | Primary Target Repository | Secondary Target Repository | Domain |
| :--- | :--- | :--- | :--- |
| **Rust / Rust 2024** | `rust-lang/rust` | `tokio-rs/tokio` | Systems / Async Runtime |
| **JavaScript / JSX** | `facebook/react` | `expressjs/express` | Web Framework |
| **TypeScript / TSX** | `microsoft/vscode` | `prisma/prisma` | IDE / ORM |
| **Python** | `psf/black` | `huggingface/transformers` | Formatter / ML |
| **Java** | `spring-projects/spring-boot` | `apache/kafka` | Enterprise / Data |
| **C / C++** | `redis/redis` | `electron/electron` | Database / Runtime |
| **Go** | `gin-gonic/gin` | `kubernetes/kubernetes` | Web / Cloud Infrastructure |
| **JSON / JSONC** | `schemastore/schemastore` | `DefinitelyTyped` | Schema / Type Definitions |
| **C#** | `dotnet/runtime` | `dotnet/roslyn` | Runtime / Compiler |
| **Ruby** | `ruby/ruby` | `rails/rails` | Interpreter / Framework |
| **PHP** | `php/php-src` | `laravel/framework` | Interpreter / Framework |

## 4. Benchmark Execution Workspace (`benchmark_workspace`)

Empirical benchmarking is executed in an isolated workspace (`benchmark_workspace/`) containing representative differential fixtures:

* `micro_edit/`: 1–3 line variable and configuration adjustments.
* `medium_refactor/`: Function extraction, variable rename, and signature migrations.
* `large_multi_file/`: Cross-file class movement, call graph DAG changes, and contract safety checks.
* `merge_conflict/`: Non-overlapping adjacent AST additions.

## 5. Formal Metric Definitions & Evaluation Formulas

### 5.1 Noise Suppression Ratio ($NSR$)

$$NSR = 1 - \frac{\text{DiffLines}_{\text{Tool}}}{\text{DiffLines}_{\text{Git}}}$$

### 5.2 Token Compression Ratio ($TCR$)

$$TCR = 1 - \frac{\text{Tokens}_{\text{symtrace}}}{\text{Tokens}_{\text{UnifiedDiff}}}$$

### 5.3 Warm Cache Speedup ($S_{\text{CAS}}$)

$$S_{\text{CAS}} = \frac{T_{\text{cold}}}{T_{\text{warm}}}$$

### 5.4 Resource & Scalability Metrics

* **Latency ($L$):** Wall-clock execution time (ms).
* **Peak Resident Set Size ($\text{RSS}_{\text{peak}}$):** Maximum physical RAM consumed in Megabytes (MB).
* **Throughput:** AST nodes processed per millisecond.
