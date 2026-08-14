# Empirical Benchmarking Protocol & Research Plan for `symtrace`

**Document Title:** Robust Structural Benchmarking Scheme for `symtrace`, `git diff`, and Open-Source AST Diff Tools  
**Author:** Jash Thakkar

**Target Tool:** `symtrace` (v0.4.5)  
**Date:** August 2026  
**Status:** Peer-Review Ready Benchmark Specification  

## Executive Summary

Software diffing is fundamental to version control, code review, program analysis, and automated program repair. Traditional line-based diff tools (e.g., `git diff` using Myers, Histogram, or Patience algorithms) treat source code as flat text streams. This line-based paradigm produces high diff noise during formatting, comment, or whitespace changes, and fails to recognize structural refactorings such as function relocation (`MOVE`) or symbol renaming (`RENAME`).

`symtrace` is a deterministic, AST-based semantic diff engine written in Rust that operates directly on Abstract Syntax Trees via Tree-sitter parsers, utilizing a multi-stage AST node matching engine, BLAKE3 node identity hashing, thread-local arena recycling (`BumpaloRecycler`), and a single-pass global graph index (`GlobalNodeIndex`).

This document defines a **comprehensive, research-grade empirical benchmarking protocol** designed to evaluate `symtrace` against state-of-the-art line-based and structural diff tools (`git diff`, `git-delta`, `difftastic`, `GumTree`, `GitHub Semantic`) across large-scale open-source repositories and verified refactor datasets.

## 1. Research Objectives & Hypotheses

The benchmarking framework addresses five primary Research Questions (RQs):

```
                       ┌──────────────────────────────────────────────┐
                       │           Research Objectives (RQs)          │
                       └──────────────────────┬───────────────────────┘
                                              │
         ┌──────────────────┬─────────────────┼──────────────────┬──────────────────┐
         ▼                  ▼                 ▼                  ▼                  ▼
    ┌──────────┐      ┌──────────┐      ┌──────────┐       ┌──────────┐       ┌──────────┐
    │   RQ1    │      │   RQ2    │      │   RQ3    │       │   RQ4    │       │   RQ5    │
    │  Noise   │      │ Refactor │      │  Runtime │       │  Merge   │       │ Security │
    │ Filter   │      │ Tracking │      │ Scaling  │       │ Driver   │       │ & SARIF  │
    └──────────┘      └──────────┘      └──────────┘       └──────────┘       └──────────┘
```

### RQ1: Noise Suppression & Noise-to-Signal Ratio

* **Hypothesis $H_1$:** `symtrace` reduces diff noise on format-only, comment-only, and import-reordering commits by $\ge 95\%$ compared to line-based `git diff` engines, yielding zero false-positive semantic operations in `--logic-only` mode.

### RQ2: Refactoring Operation Classification Accuracy

* **Hypothesis $H_2$:** `symtrace` achieves significantly higher $F_1$-scores ($>0.92$) than `difftastic` and `GumTree` in detecting cross-file `MOVE` and `RENAME` operations on real-world multi-file commits, due to its single-pass $O(N \log N)$ `GlobalNodeIndex`.

### RQ3: Computational Performance & Memory Efficiency

* **Hypothesis $H_3$:** Through `BumpaloRecycler` arena allocation and zero-copy Git OID warm-caching, `symtrace` scales sub-linearly with commit size, outperforming `GumTree` by $\ge 10\times$ in wall-clock latency and peak RSS memory utilization on large repositories ($>10,000$ AST nodes).

### RQ4: Native 3-Way AST Merge Conflict Reduction

* **Hypothesis $H_4$:** The `symtrace merge-driver` eliminates $\ge 80\%$ of text-based merge conflicts caused by concurrent non-overlapping refactorings (e.g., function reordering or variable renames in adjacent scopes) compared to Git's default `ort` driver.

### RQ5: Downstream Program Analysis & Code Scanning Integration

* **Hypothesis $H_5$:** Structurally mapped SARIF v2.1.0 output produced by `symtrace` reduces false alerts in downstream automated code scanning (e.g., GitHub Code Scanning) by filtering out non-semantic line offset shifts.

## 2. Benchmark Tool Suite & Baseline Taxonomy

The evaluation framework compares `symtrace` against a balanced spectrum of industrial baselines, academic standards, and modern tree-based diff engines:

| Tool Category | Tool Name | Parsing Model | Language Scope | Underlying Algorithm / Engine |
| :--- | :--- | :--- | :--- | :--- |
| **Line-Based Baselines** | `git diff (Myers)` | Flat Line Tokens | Language-Agnostic | LCS / Myers $O(ND)$ Algorithm |
| | `git diff --histogram` | Flat Line Tokens | Language-Agnostic | Low-frequency line matching |
| | `git diff --patience` | Flat Line Tokens | Language-Agnostic | Unique line sequence matching |
| | `git-delta` | Line + Syntax Highlight | Language-Agnostic | Line-based diff with terminal styling |
| **AST / Structural Tools** | `symtrace` (v0.4.5) | Concrete & Abstract AST | 13 Languages | 6-Stage BLAKE3 AST Graph Engine |
| | `difftastic` | Structural / Tree-sitter | 30+ Languages | Dijkstra shortest-path on AST graph |
| | `GumTree` (v3.0) | Fine-Grained AST | Java, C, JS, Python | Top-down / Bottom-up AST Subtree Match |
| | `GitHub Semantic` | AST / Haskell | TS, Python, Go, Ruby | Tree-sitter AST diffing engine |
| **Merge Drivers** | `git merge (ort)` | Line-Based 3-Way | Language-Agnostic | Ostensibly Recursive's Twin driver |
| | `symtrace merge-driver` | 3-Way AST Driver | 13 Languages | Disjoint AST Mutation Combinator |

## 3. Representative Open-Source Repository Corpus

To ensure generalizability, the benchmarking corpus spans **13 programming languages** supported by `symtrace`, selecting top-tier open-source projects with high refactoring frequency, multi-year commit histories, and diverse code structure sizes.

### 3.1 Open-Source Project Target Matrix

| Language | Primary Target Repository | Secondary Target Repository | Domain | Avg. Commits Analyzed |
| :--- | :--- | :--- | :--- | :--- |
| **Rust / Rust 2024** | `rust-lang/rust` | `tokio-rs/tokio` | Compiler / Systems | 500 Commits |
| **JavaScript / JSX** | `facebook/react` | `expressjs/express` | Web Framework | 500 Commits |
| **TypeScript / TSX** | `microsoft/vscode` | `prisma/prisma` | IDE / ORM | 500 Commits |
| **Python** | `python/cpython` | `huggingface/transformers` | Interpreter / ML | 500 Commits |
| **Java** | `spring-projects/spring-boot` | `apache/kafka` | Enterprise / Data | 500 Commits |
| **C** | `torvalds/linux` | `redis/redis` | Operating System / DB | 500 Commits |
| **C++** | `llvm/llvm-project` | `electron/electron` | Compiler Infrastructure | 500 Commits |
| **Go** | `golang/go` | `kubernetes/kubernetes` | Runtime / Cloud | 500 Commits |
| **C#** | `dotnet/runtime` | `dotnet/roslyn` | Runtime / Compiler | 500 Commits |
| **Ruby** | `ruby/ruby` | `rails/rails` | Interpreter / Framework | 300 Commits |
| **PHP** | `php/php-src` | `laravel/framework` | Interpreter / Framework | 300 Commits |
| **JSON / JSONC** | `schemastore/schemastore` | `DefinitelyTyped` | Schema / Type Defs | 200 Commits |

## 4. Formal Metric Definitions & Evaluation Formulas

### 4.1 Operation Classification Accuracy ($P, R, F_1$)

For classified semantic operations ($O \in \{\text{MOVE}, \text{RENAME}, \text{MODIFY}, \text{INSERT}, \text{DELETE}\}$):

$$\text{Precision } (P_O) = \frac{TP_O}{TP_O + FP_O}$$

$$\text{Recall } (R_O) = \frac{TP_O}{TP_O + FN_O}$$

$$F_1\text{-Score } (F_{1, O}) = 2 \cdot \frac{P_O \cdot R_O}{P_O + R_O}$$

Where:

* $TP_O$: Correctly identified operation $O$ matching ground truth.
* $FP_O$: Incorrect operation detected by the tool where no structural operation occurred.
* $FN_O$: Ground-truth operation missed by the tool.

### 4.2 Noise Suppression Ratio ($NSR$)

Evaluates a tool's resilience to non-functional changes (formatting, comments, whitespace):

$$NSR = 1 - \frac{\text{DiffLines}_{\text{Tool}}}{\text{DiffLines}_{\text{Git}}}$$

Where $NSR = 1.0$ (or $100\%$) indicates total suppression of non-semantic diff noise.

### 4.3 Merge Conflict Reduction Rate ($MCRR$)

Measures the reduction in merge conflict markers when performing rebases/merges:

$$MCRR = \frac{C_{\text{git}} - C_{\text{symtrace}}}{C_{\text{git}}}$$

Where $C_{\text{git}}$ and $C_{\text{symtrace}}$ are the number of conflicting files encountered during automated test merges.

### 4.4 Resource & Scalability Metrics

* **Latency ($L$):** Wall-clock time (milliseconds) measured at $P_{50}, P_{95}$, and $P_{99}$ percentiles using `hyperfine`.
* **Peak Resident Set Size ($\text{RSS}_{\text{peak}}$):** Maximum physical memory consumed in Megabytes (MB).
* **Memory Allocation Intensity ($A_{\text{bytes}}$):** Total bytes allocated on heap during execution (monitored via `dtrace` / `valgrind` / `DHAT`).
