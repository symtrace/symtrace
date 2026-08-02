# Change Log

All notable changes to the "symtrace-vscode" extension will be documented in this file.

## [0.3.0] - 2026-08-02

### Added
- Expanded language support in VS Code: C, C++, Go, and JSON (`.c`, `.h`, `.cpp`, `.hpp`, `.go`, `.json`).
- Added extension settings for `--path` glob filtering (`symtrace.pathGlob`) and `--color` terminal settings (`symtrace.color`).
- Improved welcome view with updated language matrix.
- Updated compatibility with Symtrace core engine v0.3.0.

## [0.2.0] - 2026-03-20

### Added
- Activity bar integration with dedicated Symtrace panel and welcome view
- Full webview report with collapsible file cards and similarity bars
- Inline editor decorations for semantic operations (5 color-coded types: INSERT, DELETE, MODIFY, MOVE, RENAME)
- Git commit picker with branch/tag support (compare two commits or commit with parent)
- Auto-download binary from GitHub releases with cross-platform archive extraction
- Side-by-side diff view via `git show` content provider
- Content Security Policy enforcement in webview (nonce-based)
- Tree view with classification badge, summary, file nodes, cross-file events, and performance metrics
- Cancellable analysis with progress notification support
- Settings for binary auto-download (`symtrace.autoDownloadBinary`)
- 4-tier binary resolution strategy (config path → PATH → cached → GitHub releases)

### Changed
- Improved tree view structure with better organization of diff results
- Enhanced webview UI with collapsible sections and visual similarity indicators

## [0.1.0] - Initial Release

### Added
- Basic commit comparison functionality
- JSON output parsing from symtrace CLI
- Commands for comparing commits
- Tree view for displaying diff results
