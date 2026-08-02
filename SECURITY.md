# symtrace Security Policy & Audit Report

**Last audited:** 2026-08-02 (v0.3.0 Production Release Audit)  
**Audit scope:** Entire v0.3.0 codebase (`src/*.rs`, `tests/`, `fuzz/`, `Cargo.toml`, `.github/workflows/release.yml`) — full manual review, static analysis, property-based testing, and fuzzing evaluation.

---

## Executive Summary

`symtrace` is a **local-only, offline, privacy-respecting** CLI tool.
It has **zero network capabilities**, collects **zero telemetry**, enforces **zero unsafe Rust code**, and processes data **only as explicitly requested** by the user.

| Property | Status |
|----------|--------|
| Network access | **None** — zero HTTP, TCP, UDP, or DNS crates |
| Telemetry / analytics | **None** — zero tracking or phoning home |
| Data exfiltration | **None** — output is written exclusively to stdout/stderr or piped to `$PAGER` |
| Environment variable access | **Limited** — reads `XDG_CACHE_HOME`, `LOCALAPPDATA`, `HOME`, `USERPROFILE`, `GIT_PAGER`, `PAGER`, `NO_COLOR` |
| Shell process execution | **Isolated** — `$PAGER` executed via explicit argument vector (no shell string wrapper) |
| Unsafe Rust code | **Denied** — `#![deny(unsafe_code)]` enforced in `Cargo.toml` |
| File writes outside cache | **None** — writes only to external user cache directory |
| Cache directory permissions | **Restricted** — Unix: `0o700` (owner-only); prevents shared-system leakage |
| Cache deserialization | **Bounded & Keyed** — 20 MiB limit, versioned envelope, BLAKE3 blob OID + `limits_hash` key |
| Parser resources | **Guarded & Fuzzed** — configurable bounds on file size, node count, depth, timeout; fuzzed via `cargo-fuzz` |
| Property testing | **Verified** — `proptest` suite asserts diff symmetry, determinism, and hash invariants |
| Supply Chain Provenance | **Automated** — Keyless Cosign OIDC signing, SPDX SBOM (`symtrace.spdx.json`), GitHub Artifact Attestations |

---

## 1. Ethical Data Processing

### What data does symtrace access?

`symtrace` reads **only** local Git repository data that the user explicitly points it to via CLI arguments:

- Git commit trees, blob objects, working tree files, and index staging area (via `libgit2`)
- Source code file contents across 9 supported formats (Rust, JS, TS, Python, Java, C, C++, Go, JSON) to build ASTs (via `tree-sitter`)

### What does symtrace do with the data?

1. Parses source code into Abstract Syntax Trees (ASTs) using bumpalo arena allocations
2. Computes structural BLAKE3 hashes for 5-phase diff matching
3. Outputs a semantic diff report to stdout or interactive shell pager

### What does symtrace NOT do?

- **Does NOT transmit any data** over the network — zero networking dependencies (`reqwest`, `hyper`, `http`, `curl` absent)
- **Does NOT collect telemetry** — no analytics, usage metrics, crash reports, or tracking
- **Does NOT invoke shell wrappers** — `$PAGER` / `$GIT_PAGER` spawned directly via explicit argument vectors
- **Does NOT read files outside** the specified Git repository or local config paths
- **Does NOT modify** the Git repository in any way (read-only operations)

---

## 2. Comprehensive v0.3.0 Audit Findings

### 2.1 Shell Pager Process Boundary Isolation — **PASS**
- **Location**: `src/pager.rs`
- **Audit Findings**: Output piping to `$GIT_PAGER` or `$PAGER` uses `std::process::Command::new` by parsing whitespace-separated argument vectors directly. Avoids `sh -c` or `cmd.exe /c` shell execution, preventing command injection attacks via shell metacharacters. Broken pipe errors on early pager exit (`q` key) are handled gracefully without panics.

### 2.2 TOML Configuration Deserialization — **PASS**
- **Location**: `src/config.rs`
- **Audit Findings**: Local `.symtracerc` or `symtrace.toml` configuration files are parsed via strongly typed `toml::from_str`. CLI options strictly override configuration settings. Invalid configuration values fall back safely to hardcoded default guardrails.

### 2.3 AST Cache Key Limits Hash Invalidation — **PASS**
- **Location**: `src/ast_cache.rs`
- **Audit Findings**: `CacheKey` incorporates `limits_hash: u64` (a BLAKE3 hash of `ParserLimits`). Changing parser CLI flags between runs automatically invalidates cached ASTs, preventing stale cache reuse under altered security bounds.

### 2.4 Bounded TreeCache LRU Capacity — **PASS**
- **Location**: `src/incremental_parse.rs`
- **Audit Findings**: Tree-sitter tree objects are held in a thread-safe `LruCache` bounded to a default capacity of 128 (expandable to 500). Memory consumption stays strictly bounded across large repository diffs.

### 2.5 Depth-Bounded Symbol Name Extraction — **PASS**
- **Location**: `src/symbol_tracking.rs`
- **Audit Findings**: Symbol identifier extraction uses a 5-level Breadth-First Search (BFS) queue. Non-symbol child subtrees are traversed up to depth 5 to locate identifier tokens reliably without stack overflow or infinite recursion risk.

### 2.6 Zero Unsafe Code Policy — **PASS**
- **Location**: `Cargo.toml` (`[lints.rust] unsafe_code = "deny"`)
- **Audit Findings**: Entire application codebase compiles with `#![deny(unsafe_code)]`. Memory safety is enforced by the Rust compiler.

---

## 3. Dependency Security Assessment

| Crate | Version | Risk | Audit Assessment & Notes |
|-------|---------|------|--------------------------|
| `clap` | `=4.5.60` | Low | Audited CLI parser; **version pinned** |
| `git2` | `=0.19.0` | Low | libgit2 Rust bindings; mature, safe API; **pinned** |
| `tree-sitter` | `=0.25.10` | Low | Safe wrapper around C parser runtime; **pinned** |
| `tree-sitter-rust` | `=0.24.0` | Low | Rust grammar parser; **pinned** |
| `tree-sitter-javascript` | `=0.25.0` | Low | JavaScript grammar parser; **pinned** |
| `tree-sitter-typescript` | `=0.23.2` | Low | TypeScript grammar parser; **pinned** |
| `tree-sitter-python` | `=0.25.0` | Low | Python grammar parser; **pinned** |
| `tree-sitter-java` | `=0.23.5` | Low | Java grammar parser; **pinned** |
| `tree-sitter-c` | `=0.23.4` | Low | C grammar parser; **pinned** |
| `tree-sitter-cpp` | `=0.23.4` | Low | C++ grammar parser; **pinned** |
| `tree-sitter-go` | `=0.23.4` | Low | Go grammar parser; **pinned** |
| `tree-sitter-json` | `=0.24.8` | Low | JSON grammar parser; **pinned** |
| `blake3` | `=1.8.3` | Low | Cryptographic SIMD hash; audited; **pinned** |
| `serde` / `serde_json` | `=1.0.228` / `=1.0.149` | Low | Standard serialization; **pinned** |
| `bincode` | `=1.3.3` | Low | Bounded deserialization (`with_limit(20MiB)`); **pinned** |
| `rayon` | `=1.11.0` | Low | Parallelism framework; **pinned** |
| `lru` | `=0.12.5` | Low | In-memory LRU cache; **pinned** |
| `bumpalo` | `=3.20.2` | Low | Arena allocator; safe API; **pinned** |
| `globset` | `=0.4.15` | Low | Glob pattern matcher; **pinned** |
| `toml` | `=0.8.23` | Low | Safe TOML deserializer; **pinned** |
| `colored` | `=2.2.0` | Low | ANSI terminal styling; **pinned** |
| `anyhow` | `=1.0.102` | Low | Error handling; **pinned** |
| `proptest` | `=1.5.0` | Low | (dev-dependency) Property testing framework; **pinned** |

**Supply Chain Hardening**:
- All dependencies are strictly version-pinned (`=x.y.z`).
- `deny.toml` is configured for automated checking via `cargo-deny`.
- CI/CD workflow embeds SPDX SBOM (`symtrace.spdx.json`), Sigstore Cosign keyless blob signatures (`.sig`), and GitHub Artifact Attestations.

---

## 4. Reporting Vulnerabilities

If you discover a security vulnerability in `symtrace`, please report it by opening an issue on GitHub (`https://github.com/JashT14/symtrace/issues`) or contacting the maintainer directly.

---

## 5. Audit Status Summary

**FINAL AUDIT VERDICT**: **APPROVED FOR PRODUCTION RELEASE (v0.3.0)** — Zero Critical, High, or Medium security vulnerabilities detected.
