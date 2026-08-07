import * as vscode from "vscode";
import { resolveBinary } from "./binary";
import { SymtraceRunner } from "./runner";
import { SymtraceTreeProvider } from "./treeview/SymtraceTreeProvider";
import { DiffPanel } from "./webview/DiffPanel";
import { pickTwoCommits, pickCommitWithParent } from "./commitPicker";
import { getConfig } from "./config";
import { applyDecorations, clearDecorations, onEditorChange } from "./decorations";
import { Logger } from "./logger";
import { GitContentProvider, GIT_SCHEME } from "./gitContentProvider";

let logger: Logger;
let runner: SymtraceRunner;
let treeProvider: SymtraceTreeProvider;

export async function activate(
  context: vscode.ExtensionContext
): Promise<void> {
  logger = new Logger();
  context.subscriptions.push({ dispose: () => logger.dispose() });

  // Resolve symtrace binary (PATH -> config -> cached download -> GitHub)
  const binaryPath = await resolveBinary(context, logger);
  if (!binaryPath) {
    // Register commands that show helpful messages
    context.subscriptions.push(
      vscode.commands.registerCommand("symtrace.compareTwoCommits", () =>
        vscode.window.showErrorMessage(
          'Symtrace binary not found. Install via "cargo install symtrace" and reload.'
        )
      ),
      vscode.commands.registerCommand("symtrace.compareWithParent", () =>
        vscode.window.showErrorMessage(
          'Symtrace binary not found. Install via "cargo install symtrace" and reload.'
        )
      ),
      vscode.commands.registerCommand("symtrace.refreshTreeView", () => {}),
      vscode.commands.registerCommand("symtrace.clearResults", () => {}),
      vscode.commands.registerCommand("symtrace.showOperationDiff", () => {})
    );
    return;
  }

  runner = new SymtraceRunner(binaryPath, logger);

  // Register git content provider for diff views
  const gitProvider = new GitContentProvider();
  context.subscriptions.push(
    vscode.workspace.registerTextDocumentContentProvider(GIT_SCHEME, gitProvider)
  );

  // Tree view
  treeProvider = new SymtraceTreeProvider();
  const treeView = vscode.window.createTreeView("symtrace.resultsTree", {
    treeDataProvider: treeProvider,
    showCollapseAll: true,
  });
  context.subscriptions.push(treeView);

  // Commands
  context.subscriptions.push(
    vscode.commands.registerCommand("symtrace.compareTwoCommits", () =>
      compareTwoCommits(context)
    ),
    vscode.commands.registerCommand("symtrace.compareWithParent", () =>
      compareWithParent(context)
    ),
    vscode.commands.registerCommand("symtrace.refreshTreeView", () =>
      treeProvider.refresh()
    ),
    vscode.commands.registerCommand("symtrace.clearResults", () => {
      treeProvider.clear();
      clearDecorations();
      vscode.commands.executeCommand("setContext", "symtrace.hasResults", false);
    }),
    vscode.commands.registerCommand(
      "symtrace.showOperationDiff",
      async (filePath: string, commitA: string, commitB: string, repoPath: string) => {
        const shortA = commitA.substring(0, 7);
        const shortB = commitB.substring(0, 7);
        const oldUri = GitContentProvider.buildUri(repoPath, commitA, filePath, shortA);
        const newUri = GitContentProvider.buildUri(repoPath, commitB, filePath, shortB);
        const title = `${filePath} (${shortA} vs ${shortB})`;
        await vscode.commands.executeCommand("vscode.diff", oldUri, newUri, title);
      }
    )
  );

  // Track editor changes for decorations
  context.subscriptions.push(
    vscode.window.onDidChangeActiveTextEditor((editor) =>
      onEditorChange(editor)
    )
  );

  vscode.commands.executeCommand("setContext", "symtrace.hasResults", false);
  logger.info(`Symtrace activated (binary: ${binaryPath})`);
}

async function compareTwoCommits(
  context: vscode.ExtensionContext
): Promise<void> {
  const repoPath = getRepoPath();
  if (!repoPath) {
    return;
  }

  const commits = await pickTwoCommits(repoPath);
  if (!commits) {
    return;
  }

  await runAndDisplay(context, repoPath, commits.commitA, commits.commitB);
}

async function compareWithParent(
  context: vscode.ExtensionContext
): Promise<void> {
  const repoPath = getRepoPath();
  if (!repoPath) {
    return;
  }

  const commits = await pickCommitWithParent(repoPath);
  if (!commits) {
    return;
  }

  await runAndDisplay(context, repoPath, commits.commitA, commits.commitB);
}

async function runAndDisplay(
  context: vscode.ExtensionContext,
  repoPath: string,
  commitA: string,
  commitB: string
): Promise<void> {
  const config = getConfig();

  const result = await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: "Symtrace: Analyzing commits...",
      cancellable: true,
    },
    async (_progress, token) => {
      return runner.run(repoPath, commitA, commitB, config, token);
    }
  );

  if (!result) {
    return;
  }

  // Update tree view
  treeProvider.setData(result, repoPath);
  vscode.commands.executeCommand("setContext", "symtrace.hasResults", true);

  // Open webview
  DiffPanel.createOrShow(context.extensionUri, result);

  // Apply inline decorations
  applyDecorations(result);
}

function getRepoPath(): string | undefined {
  if (vscode.window.activeTextEditor) {
    const folder = vscode.workspace.getWorkspaceFolder(vscode.window.activeTextEditor.document.uri);
    if (folder) {
      return folder.uri.fsPath;
    }
  }
  const folder = vscode.workspace.workspaceFolders?.[0];
  if (!folder) {
    vscode.window.showErrorMessage("No workspace folder open.");
    return undefined;
  }
  return folder.uri.fsPath;
}

export function deactivate(): void {
  clearDecorations();
}
