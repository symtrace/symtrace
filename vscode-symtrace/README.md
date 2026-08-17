<p align="center">
  <img src="https://raw.githubusercontent.com/symtrace/symtrace/main/vscode-symtrace/media/symtrace-banner.jpg" alt="symtrace banner" width="700">
</p>

# symtrace for VS Code (v0.5.0)

Semantic diff viewer powered by AST analysis. Compare Git commits to see **what semantically changed** - functions moved, renamed, modified, inserted, or deleted - rather than just which lines changed.

Built on the [symtrace](https://github.com/symtrace/symtrace) CLI engine (Rust).

> [!WARNING]
> **Active Development & Work in Progress Notice:**
> The VS Code Extension (`vscode-symtrace`) is currently under **active development**, and some known integration bugs and edge-case issues are still being resolved. It is **recommended not to package or generate `.vsix` from this extension directory for local testing** at this time. Please use the standalone `symtrace` CLI engine directly for production semantic diffing and automated quality checks.

## Features

- **Semantic Operations** - See MOVE, RENAME, MODIFY, INSERT, DELETE at the AST node level
- **Side-by-side Diff** - Click any operation to see old vs new file content in VS Code's diff editor
- **Commit Classification** - Automatic labeling (feature, bugfix, refactor, cleanup, formatting_only, mixed)
- **Cross-file Tracking** - Detect symbols that move or rename across files
- **Refactor Detection** - Identify extract method, move method, rename variable patterns
- **Similarity Scoring** - Per-operation similarity percentage with intensity rating (low/medium/high)
- **Inline Decorations** - See semantic annotations (`[INSERTED]`, `[DELETED]`, etc.) directly in your editor
- **Logic-only Mode** - Ignore comment and whitespace changes with `--logic-only`
- **White-Mode Signed HTML Report** - View full visual reports with embedded BLAKE3 digital verification signatures and PDF export support
- **Performance Metrics** - View parse time, diff time, and node counts in the sidebar
- **Activity Bar Integration** - Dedicated symtrace panel in the VS Code sidebar with SVG icon
- **Git Commit Picker** - Interactive QuickPick UI for selecting commits from your git history
- **Auto-download Binary** - 4-tier binary resolution with automatic GitHub releases download (`v0.5.0`)
- **Cancellable Analysis** - Progress notification with cancellation support during analysis

## Supported Languages

| Language | Extensions |
| :--- | :--- |
| **Rust / Rust 2024** | `.rs` |
| **JavaScript / JSX** | `.js`, `.jsx`, `.mjs`, `.cjs` |
| **TypeScript / TSX** | `.ts`, `.tsx` |
| **Python** | `.py`, `.pyi` |
| **Java** | `.java` |
| **C** | `.c`, `.h` |
| **C++** | `.cpp`, `.hpp`, `.cc`, `.cxx` |
| **Go** | `.go` |
| **JSON** | `.json`, `.jsonc` |
| **C#** | `.cs` |
| **Ruby** | `.rb` |
| **PHP** | `.php` |

## Getting Started

1. Install the extension from the VS Code Marketplace
2. Open a Git repository in VS Code
3. Click the **symtrace** icon in the Activity Bar (left sidebar)
4. Click **"Compare Two Commits"** or **"Compare Commit with Parent"** in the welcome view
5. Select commits from the picker
6. View results:
   - **Sidebar tree** - Browse operations by file, see summary, classification, refactor patterns, cross-file events, and performance
   - **Diff view** - Click any operation to see side-by-side file diff
   - **Webview panel** - Full interactive white-mode report opens automatically
   - **Inline decorations** - Annotations appear in the editor gutter

## Requirements

The `symtrace` CLI binary is required. The extension resolves it using a 4-tier strategy:

| Tier | Source | Description |
| :--- | :--- | :--- |
| 1 | Config path | Explicit path from `symtrace.binaryPath` setting |
| 2 | System PATH | `symtrace` found via `which`/`where` |
| 3 | Cached download | Previously downloaded binary in extension storage |
| 4 | GitHub releases | Auto-download from `symtrace/symtrace` releases (prompted) |

You can also install manually:

```bash
cargo install symtrace
```

### Platform Support

Auto-download supports the following targets:

| Platform | Architecture |
| :--- | :--- |
| Windows | x86_64 |
| Linux | x86_64, aarch64 |
| macOS | x86_64, aarch64 (Apple Silicon) |

## Extension Settings

| Setting | Default | Description |
| :--- | :--- | :--- |
| `symtrace.binaryPath` | `""` | Explicit path to the symtrace binary |
| `symtrace.logicOnly` | `false` | Ignore comments and whitespace changes |
| `symtrace.pathGlob` | `""` | Filter changed files matching glob pattern (`--path`) |
| `symtrace.color` | `"auto"` | Terminal color controls (`auto`, `always`, `never`) |
| `symtrace.maxFileSize` | `5242880` | Max file size in bytes (5 MiB) |
| `symtrace.maxAstNodes` | `200000` | Max AST nodes per file |
| `symtrace.maxRecursionDepth` | `2048` | Max parser recursion depth |
| `symtrace.parseTimeoutMs` | `2000` | Per-file parse timeout in ms (0 = disabled) |
| `symtrace.noIncremental` | `false` | Disable incremental parsing |
| `symtrace.autoDownloadBinary` | `true` | Auto-download binary from GitHub releases |

## Commands

| Command | Description |
| :--- | :--- |
| `symtrace: Compare Two Commits` | Select two commits to compare semantically |
| `symtrace: Compare Commit with Its Parent` | Compare a single commit against its parent |
| `symtrace: Show Operation Diff` | Open side-by-side diff for a specific operation |
| `symtrace: Refresh` | Refresh the results tree view |
| `symtrace: Clear Results` | Clear all results, decorations, and diff panels |

## How It Works

symtrace uses **tree-sitter** to parse source files into ASTs, then applies a **5-phase matching algorithm** with **BLAKE3 hashing** to identify semantic changes:

1. **Exact hash match** - Identical subtrees (move detection)
2. **Structural match** - Same shape, different content (renames/modifications)
3. **Similarity scoring** - Composite similarity with complexity analysis
4. **Leftover old nodes** - Unmatched old nodes become deletes
5. **Leftover new nodes** - Unmatched new nodes become inserts

This produces deterministic, meaningful diffs that understand code structure rather than treating files as flat text.

### Architecture

```
VS Code Extension (v0.5.0)
       │
       ├── extension.ts         ← Activation, command registration
       ├── binary.ts            ← 4-tier binary resolution + GitHub auto-download (v0.5.0)
       ├── runner.ts            ← Spawns symtrace CLI, parses JSON output
       ├── config.ts            ← Reads VS Code settings → CLI flags
       ├── commitPicker.ts      ← Git log QuickPick UI (two commits / with parent)
       ├── decorations.ts       ← Inline editor annotations (5 color-coded types)
       ├── gitContentProvider.ts ← TextDocumentContentProvider for diff views
       ├── treeview/
       │   ├── SymtraceTreeProvider.ts  ← TreeDataProvider for sidebar
       │   └── treeItems.ts             ← Node classes (summary, file, op, etc.)
       └── webview/
           └── DiffPanel.ts     ← Full White-Mode HTML report with CSP nonce
```

## Release Notes

### 0.5.0

- Aligned extension binary downloader with `v0.5.0` GitHub releases (`symtrace/symtrace`)
- Full support for adaptive granularity, call graph blast radius reporting, and contract violation warnings
- Added TypeScript types for `DisplayGranularity`, `BlastRadiusReport`, and `ContractViolation`

### 0.4.5

- Aligned extension binary downloader with `v0.4.5` GitHub releases (`symtrace/symtrace`)
- Full compatibility with multiset frequency Jaccard similarity and windowed AST diff schema

### 0.4.0

- Expanded language support matrix to 13 languages & formats: added C#, Ruby, PHP, and Rust 2024
- Updated Activity Bar icon integration to high-resolution SVG (`symtrace-new-icon.svg`)
- Support for White-Mode HTML visual reports with BLAKE3 cryptographic report fingerprinting
- Aligned extension binary downloader with `v0.4.0` GitHub releases (`symtrace/symtrace`)

### 0.3.0

- Expanded language support to include C (`.c`, `.h`), C++ (`.cpp`, `.hpp`), Go (`.go`), and JSON (`.json`, `.jsonc`)
- Added `symtrace.pathGlob` configuration setting for path glob pattern filtering (`--path`)
- Added `symtrace.color` configuration setting for terminal ANSI color controls (`--color`)
- Updated welcome view and sidebar tree view to display full 9-language matrix
- Aligned binary resolution and GitHub release download logic with v0.3.0 release assets (`symtrace/symtrace`)

### 0.2.0

- Activity bar integration with dedicated Symtrace panel and welcome view
- Full webview report with collapsible file cards and similarity bars
- Inline editor decorations for semantic operations (5 color-coded types)
- Git commit picker with branch/tag support (two commits or with parent)
- Auto-download binary from GitHub releases with cross-platform archive extraction
- Side-by-side diff view via `git show` content provider
- Content Security Policy enforcement in webview (nonce-based)
- Tree view with classification badge, summary, file nodes, cross-file events, and performance

### 0.1.0

- Initial release with basic commit comparison

## License

MIT
