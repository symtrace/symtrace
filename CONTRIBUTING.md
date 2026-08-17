# Contributing to symtrace (v0.5.0)

Thank you for your interest in contributing to `symtrace`.

## Getting Started

1. Fork and clone the repository (`https://github.com/symtrace/symtrace`)
2. Install [Rust](https://www.rust-lang.org/tools/install) (edition 2021+) and a C compiler (for libgit2 and tree-sitter C grammars)
3. Run `cargo build` to verify your setup
4. Run `cargo test --workspace` to ensure all 332 unit, property-based (`proptest`), and differential tests pass

See [DEVELOPMENT.md](DEVELOPMENT.md) for build system details, build targets, and release configuration.

## Making Changes

1. Create a feature branch from `main`
2. Make your changes in small, focused commits
3. Add or update tests for any new functionality (including property tests in `tests/proptests.rs` or differential tests in `tests/differential_tests.rs` where applicable)
4. Run the full validation before submitting:

```bash
# Format + lint + full test suite + release build
cargo fmt --check
cargo clippy -- -D warnings
cargo test --all
```

## Code & Security Standards

- **Formatting** — all code must pass `cargo fmt --all -- --check`
- **Strict Linting** — all clippy warnings are errors: `cargo clippy --all-targets --all-features -- -D warnings`
- **Testing** — all 332 unit, property (`proptest`), and differential integration tests must pass (`cargo test --all`)
- **Zero Unsafe Code** — `unsafe_code = "deny"` is enforced in `Cargo.toml`
- **Pinned Dependencies** — new dependencies must use exact version pinning (`=x.y.z`)
- **Language Support** — changes affecting AST parsing must verify compatibility across all 13 supported languages/formats (Rust, JavaScript, TypeScript, Python, Java, C, C++, Go, JSON, C#, Ruby, PHP, Rust 2024)

## Pull Requests

- Keep PRs focused on a single change or feature area
- Include a clear description of what changed and why
- Reference any related issue numbers
- Ensure all CI checks (format, clippy, test suite, security provenance) pass cleanly before requesting review

## Reporting Issues

- Use GitHub Issues for bug reports and feature requests
- Include steps to reproduce for bugs
- Include the output of `symtrace --version` and your operating system / environment details

## Security

See [SECURITY.md](SECURITY.md) for security policies, audit details, and vulnerability reporting.

## License

By contributing, you agree that your contributions will be licensed under the [MIT License](LICENSE).
