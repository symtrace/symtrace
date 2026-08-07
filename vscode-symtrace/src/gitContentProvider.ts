import * as vscode from "vscode";
import * as cp from "child_process";

export const GIT_SCHEME = "symtrace-git";

export class GitContentProvider implements vscode.TextDocumentContentProvider {
  provideTextDocumentContent(uri: vscode.Uri): Promise<string> {
    const params = new URLSearchParams(uri.query);
    const commit = params.get("commit");
    const filePath = params.get("file");
    const repoPath = params.get("repo");

    if (!commit || !filePath || !repoPath) {
      return Promise.resolve("// Unable to resolve file content");
    }

    return this.gitShow(repoPath, commit, filePath);
  }

  private gitShow(
    repoPath: string,
    commit: string,
    filePath: string
  ): Promise<string> {
    const normalizedPath = filePath.replace(/\\/g, "/");
    return new Promise((resolve) => {
      cp.execFile(
        "git",
        ["show", `${commit}:${normalizedPath}`],
        { cwd: repoPath, maxBuffer: 10 * 1024 * 1024, encoding: "utf8" },
        (err, stdout) => {
          if (err) {
            resolve(`// File does not exist at commit ${commit}`);
          } else {
            resolve(stdout);
          }
        }
      );
    });
  }

  static buildUri(
    repoPath: string,
    commit: string,
    filePath: string,
    label: string
  ): vscode.Uri {
    return vscode.Uri.parse(
      `${GIT_SCHEME}:/${label}/${filePath}?commit=${encodeURIComponent(commit)}&file=${encodeURIComponent(filePath)}&repo=${encodeURIComponent(repoPath)}`
    );
  }
}
