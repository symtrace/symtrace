import * as vscode from "vscode";
import * as cp from "child_process";

export interface CommitInfo {
  hash: string;
  shortHash: string;
  subject: string;
  author: string;
  date: string;
}

export interface CommitPair {
  commitA: string;
  commitB: string;
}

const DELIMITER = "<SYMTRACE_SPLIT>";

export async function pickTwoCommits(
  repoPath: string
): Promise<CommitPair | undefined> {
  const commits = await getRecentCommits(repoPath, 50);

  type OptionItem = vscode.QuickPickItem & {
    action?: "worktree" | "staged" | "commit";
    commit?: CommitInfo;
  };

  const options: OptionItem[] = [
    {
      label: "$(tools) Working Tree vs HEAD",
      description: "Compare uncommitted workspace changes against last commit",
      action: "worktree",
    },
    {
      label: "$(git-pull-request) Staged Index vs HEAD",
      description: "Compare staged git index changes against last commit",
      action: "staged",
    },
  ];

  if (commits.length > 0) {
    options.push({
      label: "--- Recent Git Commits ---",
      kind: vscode.QuickPickItemKind.Separator,
    });
    for (const c of commits) {
      options.push({
        label: `$(git-commit) ${c.shortHash}`,
        description: c.subject,
        detail: `${c.author} • ${c.date}`,
        action: "commit",
        commit: c,
      });
    }
  }

  const selectedA = await vscode.window.showQuickPick(options, {
    placeHolder: "Select base commit or workspace target to compare",
    matchOnDescription: true,
    matchOnDetail: true,
  });

  if (!selectedA) {
    return undefined;
  }

  if (selectedA.action === "worktree") {
    return { commitA: "HEAD", commitB: "WORKTREE" };
  }

  if (selectedA.action === "staged") {
    return { commitA: "HEAD", commitB: "STAGED" };
  }

  // Base commit selected, pick target commit
  const commitA = selectedA.commit!;
  const targetCommits = commits.filter((c) => c.hash !== commitA.hash);
  if (targetCommits.length === 0) {
    // Single commit repository fallback
    return { commitA: `${commitA.hash}~1`, commitB: commitA.hash };
  }

  const selectedB = await showCommitPicker(
    targetCommits,
    "Select the NEWER commit (target)"
  );
  if (!selectedB) {
    return undefined;
  }

  return { commitA: commitA.hash, commitB: selectedB.hash };
}

export async function pickCommitWithParent(
  repoPath: string
): Promise<CommitPair | undefined> {
  const commits = await getRecentCommits(repoPath, 50);

  if (commits.length === 0) {
    const fallback = await vscode.window.showWarningMessage(
      "No git commits found in history. Compare Working Tree against HEAD?",
      "Compare Working Tree"
    );
    if (fallback === "Compare Working Tree") {
      return { commitA: "HEAD", commitB: "WORKTREE" };
    }
    return undefined;
  }

  const commit = await showCommitPicker(
    commits,
    "Select a commit to compare with its parent"
  );
  if (!commit) {
    return undefined;
  }

  return { commitA: `${commit.hash}~1`, commitB: commit.hash };
}

export async function getRecentCommits(
  repoPath: string,
  count: number = 50
): Promise<CommitInfo[]> {
  return new Promise((resolve) => {
    const formatStr = `%H${DELIMITER}%h${DELIMITER}%s${DELIMITER}%an${DELIMITER}%ar`;
    cp.execFile(
      "git",
      ["log", `-n${count}`, `--format=${formatStr}`],
      { cwd: repoPath, maxBuffer: 10 * 1024 * 1024, encoding: "utf8" },
      (err, stdout) => {
        if (err || !stdout) {
          resolve([]);
          return;
        }
        const lines = stdout.split(/\r?\n/).filter((l) => l.trim().length > 0);
        const commits: CommitInfo[] = [];
        for (const line of lines) {
          const parts = line.split(DELIMITER);
          if (parts.length >= 5) {
            commits.push({
              hash: parts[0],
              shortHash: parts[1],
              subject: parts[2],
              author: parts[3],
              date: parts[4],
            });
          }
        }
        resolve(commits);
      }
    );
  });
}

async function showCommitPicker(
  commits: CommitInfo[],
  title: string
): Promise<CommitInfo | undefined> {
  const items: (vscode.QuickPickItem & { commit: CommitInfo })[] = commits.map(
    (c) => ({
      label: `$(git-commit) ${c.shortHash}`,
      description: c.subject,
      detail: `${c.author} • ${c.date}`,
      commit: c,
    })
  );

  const selected = await vscode.window.showQuickPick(items, {
    placeHolder: title,
    matchOnDescription: true,
    matchOnDetail: true,
  });

  return selected?.commit;
}
