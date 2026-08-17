# Development Guide — Symtrace v0.5.0

This document describes the production-ready build, quality verification, and release development workflow for `symtrace` v0.5.0.

## Build System

The project is configured for **maximum portability** and **production quality** across Windows, macOS, and Linux.

### Build Scripts

Three ways to build, all with identical targets and no system-specific hardcoding:

#### 1. **Windows (PowerShell)**

```powershell
.\build.ps1 -Target production    # Full production build
.\build.ps1 -Target release       # Release binary only
.\build.ps1 -Target test          # Run tests
.\build.ps1 -Target lint          # Run clippy
.\build.ps1 -Target fmt           # Format code
.\build.ps1 -Target help          # Show all targets
```

#### 2. **macOS / Linux (Bash)**

```bash
./build.sh production    # Full production build
./build.sh release       # Release binary only
./build.sh test          # Run tests
./build.sh lint          # Run clippy
./build.sh fmt           # Format code
./build.sh help          # Show all targets
```

#### 3. **Direct Cargo** (all platforms)

```bash
cargo build                    # Debug build
cargo build --release         # Release build (optimized)
cargo test --all              # Run full test suite (166 tests)
cargo clippy --all-targets --all-features -- -D warnings    # Lint with zero warnings allowed
cargo fmt --all -- --check    # Check code formatting
cargo install --path .        # Install binary globally
```

### GNU Make (optional)

If `make` is installed on your system (Linux/macOS):

```bash
make release      # Make targets are identical to build scripts
make test
make lint
make production
make help         # Show all targets
```

## Production Build

### Recommended: Full Validation

```powershell
# Windows
.\build.ps1 -Target production

# Linux/macOS
./build.sh production

# Or any platform
cargo clean && cargo fmt --all -- --check && \
  cargo clippy --all-targets --all-features -- -D warnings && \
  cargo test --all && cargo build --release
```

This runs:

1. ✓ Clean build directory
2. ✓ Format check (`rustfmt`)
3. ✓ Linter (`clippy` with warnings-as-errors)
4. ✓ Full test suite (unit + proptests + differential tests)
5. ✓ Release build (LLVM -O3 + LTO optimized)

### Binary Location

After a successful build, the binary is located at:

- **Windows:** `target\release\symtrace.exe`
- **macOS/Linux:** `target/release/symtrace`

### Release Build Configuration

All release builds use production-optimized settings from `.cargo/config.toml`:

| Setting | Value | Purpose |
| --------- | ------- | --------- |
| `opt-level` | 3 | Maximum optimization (LLVM -O3) |
| `lto` | true | Link-Time Optimization |
| `codegen-units` | 1 | Single codegen unit (slower compile, faster binary) |
| `strip` | true | Strip symbols (smaller binary) |
| `panic` | abort | Smaller runtime (abort instead of unwind) |
| `jobs` | -1 | Use all available CPU cores |

Result: Fast, small, production-grade binary with minimal runtime overhead.

## Code Quality & Testing Suite Architecture

`symtrace` v0.5.0 enforces a strict four-tier verification architecture:

```
1. Unit Tests (276 passing)       ──► Validate parser, diff engine, limits, config, and pagers
2. Property Tests (20 passing)    ──► Assert diff symmetry, determinism, and structural invariants
3. Differential Tests (36 passing)──► Assert zero false positives vs git diff baseline
4. Fuzzing (cargo-fuzz)            ──► Validate parser resource limits under adversarial inputs
```

### 1. Pre-commit Checks

Run before committing:

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test --all
```

### 2. Running the Full Test Suite

```bash
# Run unit tests
cargo test --bin symtrace

# Run differential multi-language integration tests
cargo test --test differential_tests

# Run proptest randomized invariant test suite
cargo test --test proptests

# Run all test suites
cargo test --all
```

---

## Dependency Management

| `tree-sitter-typescript` | `=0.23.2` | TypeScript language grammar |
| `tree-sitter-python` | `=0.25.0` | Python language grammar |
| `tree-sitter-java` | `=0.23.5` | Java language grammar |
| `tree-sitter-c` | `=0.23.4` | C language grammar |
| `tree-sitter-cpp` | `=0.23.4` | C++ language grammar |
| `tree-sitter-go` | `=0.23.4` | Go language grammar |
| `tree-sitter-json` | `=0.24.8` | JSON language grammar |
| `blake3` | `=1.8.3` | SIMD-optimized BLAKE3 hashing |
| `serde` / `serde_json` | `=1.0.228` / `=1.0.149` | Serde JSON serialization |
| `bincode` | `=1.3.3` | Bounded binary serialization (AST cache) |
| `rayon` | `=1.11.0` | Multi-threaded parallel processing |
| `lru` | `=0.12.5` | Bounded in-memory LRU cache |
| `bumpalo` | `=3.20.2` | Arena allocator |
| `globset` | `=0.4.15` | Path glob pattern matching |
| `toml` | `=0.8.23` | TOML config file parser (`.symtracerc`) |
| `colored` | `=2.2.0` | ANSI terminal styling |
| `anyhow` | `=1.0.102` | Error handling |
| `proptest` | `=1.5.0` | (dev-dependency) Property testing framework |

All versions are exactly pinned (`=x.y.z`) in `Cargo.toml`. `Cargo.lock` is committed for 100% reproducible builds.

## Installation for Development

```bash
# 1. Clone
git clone https://github.com/symtrace/symtrace.git
cd symtrace

# 2. Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 3. Build & Verify
cargo build --release
cargo test --all

# 4. Install binary globally
cargo install --path .
```

The binary is now available as `symtrace` from any terminal.

## Automation, Security & Release Distribution

### CI/CD Release Pipeline (`.github/workflows/release.yml`)

The automated release workflow triggers on git tag push (`v*`):

1. **GitHub Free Tier Runner Optimization**: Restricted to `ubuntu-latest` and `windows-latest` standard runners (execution time < 4 min per release).
2. **Multi-Platform Build Matrix**: Builds Linux (`x86_64`, `aarch64`) tarballs and Windows (`x86_64`) zip archives.
3. **Automated Cryptographic Provenance**:
   - Generates `SHA256SUMS.txt` for all release archives.
   - Keyless Sigstore Cosign OIDC blob signing (`cosign sign-blob`) producing `.sig` and `.pem` certs.
   - SPDX Software Bill of Materials (SBOM) via `anchore/sbom-action` (`symtrace.spdx.json`).
   - GitHub Artifact Attestation via `actions/attest-build-provenance`.
   - Immutable release publishing via `softprops/action-gh-release`.

### Universal Installers

- `install.sh`: POSIX shell installer for Linux/macOS installing binary into `~/.local/bin`.
- `install.ps1`: PowerShell installer for Windows installing binary into `%LOCALAPPDATA%\symtrace\bin` and updating `PATH`.

## Troubleshooting

### Build Failures

1. **Missing C compiler:** Install a C compiler (MSVC / Build Tools on Windows, GCC/Clang on Linux/macOS).
2. **libgit2 build errors:** Ensure active internet connection for first-time crate downloads.
3. **Disk space:** Release builds require temporary target space; run `cargo clean` to reclaim disk space.

### Test Failures

Run tests with verbose backtraces:

```bash
RUST_BACKTRACE=1 cargo test --all -- --nocapture
```

### Lint Failures

Check explicit clippy diagnostics:

```bash
cargo clippy --all-targets --all-features --message-format=short
```

## System Independence

All build configuration is portable:

- ✓ No hardcoded local paths
- ✓ Portable configuration in `.cargo/config.toml`
- ✓ Environment-independent (except optional `RUST_BACKTRACE`)
- ✓ `.gitignore` blocks scratch files and build artifacts

Safe to push to GitHub without exposing personal environment information.
