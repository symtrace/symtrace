# `symtrace` Security Policy & Audit Report - v0.4.0

**Audit Date:** August 2026 (`v0.4.0` Release Audit)  
**Audit Scope:** Entire codebase (`src/*.rs`, `tests/`, `fuzz/`, `Cargo.toml`, `.github/workflows/release.yml`) - manual review, static analysis, property-based testing, and fuzzing evaluation.

## Executive Summary

`symtrace` is a **local-only, offline, privacy-respecting** CLI code intelligence tool.
It has **zero network capabilities**, collects **zero telemetry**, enforces **zero unsafe Rust code**, and processes data **only as explicitly requested** by the user.

| Property | Status |
| :--- | :--- |
| **Network Access** | **None** - zero HTTP, TCP, UDP, or DNS dependencies |
| **Telemetry / Analytics** | **None** - zero tracking or phoning home |
| **Data Exfiltration** | **None** - output written strictly to stdout/stderr or piped to `$PAGER` |
| **Unsafe Rust Code** | **Denied** - `#![deny(unsafe_code)]` enforced in `Cargo.toml` |
| **Report Integrity** | **Cryptographic** - 64-character BLAKE3 digital fingerprint signing for HTML/PDF reports |
| **Audit Liability** | **Guarded** - includes explicit AST heuristic accuracy disclaimer notice |
| **Cache Security** | **Bounded & Keyed** - 20 MiB limit, versioned envelope, BLAKE3 blob OID + `limits_hash` key |
| **Parser Guardrails** | **Guarded & Fuzzed** - bounds on file size, node count, depth, timeout; fuzzed via `cargo-fuzz` |
| **Oversized File DoS** | **Subtree Windowed** - files >1 MiB windowed to modified line hunks ($O(\text{hunk\_size})$ complexity) |
| **3-Way Merge Security** | **Atomic Write** - validates path parameters and writes output atomically |
| **Property Testing** | **Verified** - `proptest` suite asserts diff symmetry, determinism, and hash invariants (186 passing tests) |
| **Supply Chain Provenance** | **Automated** - Keyless Cosign OIDC signing, SPDX SBOM (`symtrace.spdx.json`), GitHub Artifact Attestations |

## 1. Ethical Data Processing

### What data does `symtrace` access?

`symtrace` reads **only** local Git repository data that the user explicitly points it to via CLI arguments:

- Git commit trees, blob objects, working tree files, and index staging area (via `libgit2`).
- Source code file contents across 13 supported formats (Rust, JS, TS, Python, Java, C, C++, Go, JSON, C#, Ruby, PHP, Rust 2024) to build ASTs (via `tree-sitter`).

### What does `symtrace` do with the data?

1. Parses source code into Abstract Syntax Trees (ASTs) using `BumpaloRecycler` arena allocations.
2. Computes structural BLAKE3 hashes for 5-phase diff matching and global multi-file indexing.
3. Outputs a semantic diff report to stdout, interactive terminal TUI, or signed HTML/PDF report.

### What does `symtrace` NOT do?

- **Does NOT transmit any data** over the network - zero networking dependencies (`reqwest`, `hyper`, `http` absent).
- **Does NOT collect telemetry** - no analytics, usage metrics, crash reports, or tracking.
- **Does NOT invoke shell wrappers** - `$PAGER` / `$GIT_PAGER` spawned directly via explicit argument vectors.
- **Does NOT read files outside** the specified Git repository or local config paths.

## 2. Comprehensive Security Audit Findings

### 2.1 Compiler-Enforced Memory Safety (`#![deny(unsafe_code)]`)

- **Location**: `Cargo.toml` (`[lints.rust] unsafe_code = "deny"`)
- **Findings**: Entire application codebase compiles with `#![deny(unsafe_code)]`. Memory safety is compiler-guaranteed.

### 2.2 Shell Pager Process Boundary Isolation

- **Location**: [src/pager.rs](src/pager.rs)
- **Findings**: Output piping to `$GIT_PAGER` or `$PAGER` uses `std::process::Command::new` by parsing whitespace-separated argument vectors directly. Avoids `sh -c` or `cmd.exe /c` shell execution, preventing command injection attacks.

### 2.3 Cryptographic BLAKE3 Report Fingerprinting & Signing

- **Location**: [src/output.rs](src/output.rs)
- **Findings**: Evaluates a 64-character BLAKE3 cryptographic hash over serialized report bytes. Embeds a digital verification badge (`DIGITAL AUDIT SIGNATURE [VERIFIED]`) ensuring tamper-proof report integrity.

### 2.4 Safe Interactive TUI Inspector

- **Location**: [src/tui.rs](src/tui.rs)
- **Findings**: Built cleanly in safe Rust using `crossterm` raw mode abstractions. Terminal state is cleanly restored upon exit (`LeaveAlternateScreen`, `disable_raw_mode`) without terminal corruption.

### 2.5 Atomic 3-Way Merge File Overwrites

- **Location**: [src/merge_driver.rs](src/merge_driver.rs)
- **Findings**: `run_merge_driver` validates file paths and executes atomic file writes (`fs::write(ours_path, content)`) only after completing 3-way AST structural conflict resolution, preventing corruption.

## 3. Related Documentation

- [TECHNICAL_SPECIFICATIONS.md](TECHNICAL_SPECIFICATIONS.md) - Detailed technical specifications.
- [BENCHMARKS.md](BENCHMARKS.md) - Performance benchmark metrics.
- [CHANGELOG.md](CHANGELOG.md) - Version release history & changelog.

## 4. Reporting a Vulnerability

If you discover a security vulnerability, please report it via GitHub Private Vulnerability Reporting or reach out to me via my socials.
