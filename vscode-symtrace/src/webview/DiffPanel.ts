import * as vscode from "vscode";
import {
  DiffOutput,
  FileDiff,
  OperationRecord,
  getFilePath,
  getOpType,
  getEntityType,
  getOldLocation,
  getNewLocation,
  getSimilarityPercent,
  getCommitA,
  getCommitB,
  getTotalFiles,
  getTotalTimeMs,
  getParseTimeMs,
  getDiffTimeMs,
  getFilesProcessed,
  getNodesCompared,
  getPrimaryClass,
  getConfidenceScore,
  getCrossFileEvents,
  getRefactorPatterns,
} from "../types";

export class DiffPanel {
  public static currentPanel: DiffPanel | undefined;
  private static readonly viewType = "symtrace.diffPanel";

  private readonly panel: vscode.WebviewPanel;
  private disposables: vscode.Disposable[] = [];

  private constructor(
    panel: vscode.WebviewPanel,
    data: DiffOutput
  ) {
    this.panel = panel;
    this.panel.webview.html = DiffPanel.getHtml(data);

    this.panel.webview.onDidReceiveMessage(
      (message) => this.handleMessage(message),
      null,
      this.disposables
    );

    this.panel.onDidDispose(() => this.dispose(), null, this.disposables);
  }

  public static createOrShow(
    extensionUri: vscode.Uri,
    data: DiffOutput
  ): void {
    const column = vscode.window.activeTextEditor?.viewColumn;

    if (DiffPanel.currentPanel) {
      DiffPanel.currentPanel.panel.reveal(column);
      DiffPanel.currentPanel.panel.webview.html = DiffPanel.getHtml(data);
      return;
    }

    const commitAStr = getCommitA(data);
    const commitBStr = getCommitB(data);
    const panel = vscode.window.createWebviewPanel(
      DiffPanel.viewType,
      `Symtrace: ${commitAStr.substring(0, 7)}..${commitBStr.substring(0, 7)}`,
      column || vscode.ViewColumn.One,
      { enableScripts: true, retainContextWhenHidden: true }
    );

    DiffPanel.currentPanel = new DiffPanel(panel, data);
  }

  private handleMessage(message: { command: string; filePath?: string; line?: number }): void {
    if (message.command === "openFile" && message.filePath) {
      const uri = vscode.Uri.file(message.filePath);
      const line = (message.line ?? 1) - 1;
      vscode.window.showTextDocument(uri, {
        selection: new vscode.Range(line, 0, line, 0),
        preview: true,
      });
    }
  }

  private dispose(): void {
    DiffPanel.currentPanel = undefined;
    this.panel.dispose();
    for (const d of this.disposables) {
      d.dispose();
    }
    this.disposables = [];
  }

  private static getHtml(data: DiffOutput): string {
    const nonce = getNonce();
    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'nonce-${nonce}'; script-src 'nonce-${nonce}';">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Symtrace Diff</title>
  <style nonce="${nonce}">
    :root {
      --insert-color: var(--vscode-gitDecoration-addedResourceForeground, #3fb950);
      --delete-color: var(--vscode-gitDecoration-deletedResourceForeground, #f85149);
      --modify-color: var(--vscode-gitDecoration-modifiedResourceForeground, #d29922);
      --move-color: var(--vscode-editorInfo-foreground, #3794ff);
      --rename-color: var(--vscode-editorWarning-foreground, #cca700);
    }
    * { box-sizing: border-box; margin: 0; padding: 0; }
    body {
      font-family: var(--vscode-font-family, sans-serif);
      font-size: var(--vscode-font-size, 13px);
      color: var(--vscode-foreground);
      background: var(--vscode-editor-background);
      padding: 16px;
      line-height: 1.5;
    }
    .header {
      border-bottom: 1px solid var(--vscode-panel-border, #333);
      padding-bottom: 12px;
      margin-bottom: 16px;
    }
    .header h1 {
      font-size: 1.4em;
      font-weight: 600;
      margin-bottom: 4px;
    }
    .header .meta {
      color: var(--vscode-descriptionForeground);
      font-size: 0.9em;
    }
    .badge {
      display: inline-block;
      padding: 2px 8px;
      border-radius: 10px;
      font-size: 0.8em;
      font-weight: 600;
      margin-left: 8px;
      vertical-align: middle;
    }
    .badge.refactor { background: #1f6feb33; color: #58a6ff; }
    .badge.feature { background: #23863533; color: #3fb950; }
    .badge.bug_fix { background: #da363333; color: #f85149; }
    .badge.cleanup { background: #48484833; color: #8b949e; }
    .badge.formatting_only { background: #30363d33; color: #6e7681; }
    .badge.mixed { background: #d2992233; color: #e3b341; }

    .summary-bar {
      display: flex;
      gap: 16px;
      padding: 10px 14px;
      background: var(--vscode-editor-inactiveSelectionBackground, #264f7833);
      border-radius: 6px;
      margin-bottom: 16px;
      flex-wrap: wrap;
    }
    .summary-item {
      display: flex;
      align-items: center;
      gap: 4px;
      font-size: 0.9em;
    }
    .summary-item .dot {
      width: 8px;
      height: 8px;
      border-radius: 50%;
      display: inline-block;
    }

    .file-card {
      border: 1px solid var(--vscode-panel-border, #333);
      border-radius: 6px;
      margin-bottom: 12px;
      overflow: hidden;
    }
    .file-header {
      padding: 8px 12px;
      background: var(--vscode-sideBar-background, #1e1e1e);
      font-weight: 600;
      cursor: pointer;
      display: flex;
      align-items: center;
      gap: 6px;
      user-select: none;
    }
    .file-header:hover {
      background: var(--vscode-list-hoverBackground, #2a2d2e);
    }
    .file-header .chevron {
      transition: transform 0.15s;
      font-size: 0.8em;
    }
    .file-header .chevron.collapsed { transform: rotate(-90deg); }
    .file-body { padding: 0; }
    .file-body.hidden { display: none; }

    .op-row {
      display: flex;
      align-items: center;
      padding: 6px 12px 6px 24px;
      gap: 8px;
      border-top: 1px solid var(--vscode-panel-border, #333);
      cursor: pointer;
      transition: background 0.1s;
    }
    .op-row:hover {
      background: var(--vscode-list-hoverBackground, #2a2d2e);
    }
    .op-badge {
      font-size: 0.75em;
      font-weight: 700;
      padding: 1px 6px;
      border-radius: 3px;
      min-width: 54px;
      text-align: center;
      flex-shrink: 0;
    }
    .op-badge.INSERT  { background: #23863522; color: var(--insert-color); }
    .op-badge.DELETE  { background: #da363322; color: var(--delete-color); }
    .op-badge.MODIFY  { background: #d2992222; color: var(--modify-color); }
    .op-badge.MOVE    { background: #1f6feb22; color: var(--move-color); }
    .op-badge.RENAME  { background: #cca70022; color: var(--rename-color); }

    .op-entity {
      color: var(--vscode-descriptionForeground);
      font-size: 0.85em;
      min-width: 60px;
      flex-shrink: 0;
    }
    .op-details {
      flex: 1;
      overflow: hidden;
      text-overflow: ellipsis;
      white-space: nowrap;
    }
    .op-location {
      color: var(--vscode-descriptionForeground);
      font-size: 0.85em;
      flex-shrink: 0;
    }
    .similarity-bar-wrap {
      width: 50px;
      height: 6px;
      background: var(--vscode-progressBar-background, #333);
      border-radius: 3px;
      overflow: hidden;
      flex-shrink: 0;
    }
    .similarity-bar-fill {
      height: 100%;
      border-radius: 3px;
      transition: width 0.3s;
    }
    .similarity-label {
      font-size: 0.75em;
      color: var(--vscode-descriptionForeground);
      width: 32px;
      text-align: right;
      flex-shrink: 0;
    }

    .section {
      margin-top: 20px;
    }
    .section h2 {
      font-size: 1.1em;
      margin-bottom: 8px;
      font-weight: 600;
    }

    .refactor-row {
      padding: 4px 12px 4px 24px;
      font-size: 0.9em;
      color: var(--vscode-descriptionForeground);
      border-top: 1px solid var(--vscode-panel-border, #333);
    }
    .refactor-row .kind {
      color: #d2a8ff;
      font-weight: 600;
    }

    .cross-file-row {
      padding: 6px 12px;
      font-size: 0.9em;
      border-bottom: 1px solid var(--vscode-panel-border, #333);
    }

    .perf-footer {
      margin-top: 20px;
      padding-top: 12px;
      border-top: 1px solid var(--vscode-panel-border, #333);
      color: var(--vscode-descriptionForeground);
      font-size: 0.8em;
      display: flex;
      gap: 16px;
      flex-wrap: wrap;
    }
  </style>
</head>
<body>
  ${renderHeader(data)}
  ${renderSummaryBar(data)}
  ${data.files.map((f) => renderFileCard(f)).join("")}
  ${renderCrossFileSection(data)}
  ${renderPerformance(data)}

  <script nonce="${nonce}">
    const vscode = acquireVsCodeApi();

    document.querySelectorAll('.file-header').forEach(header => {
      header.addEventListener('click', () => {
        const body = header.nextElementSibling;
        const chevron = header.querySelector('.chevron');
        if (body) body.classList.toggle('hidden');
        if (chevron) chevron.classList.toggle('collapsed');
      });
    });

    document.querySelectorAll('.op-row[data-file]').forEach(row => {
      row.addEventListener('click', () => {
        const filePath = row.getAttribute('data-file');
        const line = parseInt(row.getAttribute('data-line') || '1', 10);
        vscode.postMessage({ command: 'openFile', filePath, line });
      });
    });
  </script>
</body>
</html>`;
  }
}

function renderHeader(data: DiffOutput): string {
  const primaryClass = getPrimaryClass(data);
  const confidenceScore = getConfidenceScore(data);
  const commitAStr = getCommitA(data);
  const commitBStr = getCommitB(data);

  const badge = (data.commitClassification || data.commit_classification)
    ? `<span class="badge ${primaryClass}">${primaryClass.replace("_", " ")} (${(confidenceScore * 100).toFixed(0)}%)</span>`
    : "";

  return `<div class="header">
    <h1>Symtrace Semantic Diff${badge}</h1>
    <div class="meta">
      ${escHtml(data.repository)} &mdash;
      <code>${escHtml(commitAStr.substring(0, 10))}</code> &rarr;
      <code>${escHtml(commitBStr.substring(0, 10))}</code>
    </div>
  </div>`;
}

function renderSummaryBar(data: DiffOutput): string {
  const s = data.summary;
  const totalFilesNum = getTotalFiles(data);
  const items = [
    { label: "Files", count: totalFilesNum, color: "#8b949e" },
    { label: "Inserts", count: s.inserts, color: "#3fb950" },
    { label: "Deletes", count: s.deletes, color: "#f85149" },
    { label: "Modifies", count: s.modifications, color: "#d29922" },
    { label: "Moves", count: s.moves, color: "#3794ff" },
    { label: "Renames", count: s.renames, color: "#cca700" },
  ];

  return `<div class="summary-bar">
    ${items.map((i) => `<div class="summary-item"><span class="dot" style="background:${i.color}"></span>${i.label}: ${i.count}</div>`).join("")}
  </div>`;
}

function renderFileCard(file: FileDiff): string {
  const filePathStr = getFilePath(file);
  const ops = file.operations.map((op) => renderOperation(op, filePathStr)).join("");
  const refactors = getRefactorPatterns(file)
    .map(
      (r) =>
        `<div class="refactor-row"><span class="kind">${escHtml(r.kind)}</span> ${escHtml(r.description)} (${(r.confidence * 100).toFixed(0)}%)</div>`
    )
    .join("");

  return `<div class="file-card">
    <div class="file-header">
      <span class="chevron">&#9662;</span>
      ${escHtml(filePathStr)}
      <span style="color:var(--vscode-descriptionForeground);font-size:0.85em;margin-left:auto">${file.operations.length} ops</span>
    </div>
    <div class="file-body">${ops}${refactors}</div>
  </div>`;
}

function renderOperation(op: OperationRecord, filePath: string): string {
  const opType = getOpType(op);
  const entityType = getEntityType(op);
  const oldLoc = getOldLocation(op);
  const newLoc = getNewLocation(op);
  const simPercent = getSimilarityPercent(op.similarity);

  const loc = newLoc ?? oldLoc ?? "";
  const lineMatch = loc.match(/L(\d+)/);
  const lineNum = lineMatch ? lineMatch[1] : "";
  const locDisplay = formatLocation(oldLoc, newLoc);

  let simBar = "";
  if (op.similarity) {
    const pct = simPercent;
    const color = pct >= 80 ? "#3fb950" : pct >= 50 ? "#d29922" : "#f85149";
    simBar = `
      <div class="similarity-bar-wrap"><div class="similarity-bar-fill" style="width:${pct}%;background:${color}"></div></div>
      <span class="similarity-label">${pct.toFixed(0)}%</span>`;
  }

  return `<div class="op-row" data-file="${escAttr(filePath)}" data-line="${escAttr(lineNum)}">
    <span class="op-badge ${opType}">${opType}</span>
    <span class="op-entity">${escHtml(entityType)}</span>
    <span class="op-details">${escHtml(op.details)}</span>
    <span class="op-location">${escHtml(locDisplay)}</span>
    ${simBar}
  </div>`;
}

function formatLocation(
  old_loc: string | undefined,
  new_loc: string | undefined
): string {
  if (old_loc && new_loc) {
    return `${old_loc} -> ${new_loc}`;
  }
  return old_loc ?? new_loc ?? "";
}

function renderCrossFileSection(data: DiffOutput): string {
  const crossEvents = getCrossFileEvents(data);
  const tracking = data.crossFileTracking || data.cross_file_tracking;
  if (!tracking || crossEvents.length === 0) {
    return "";
  }

  const symbolCount = tracking.symbolCount ?? tracking.symbol_count ?? 0;

  const rows = crossEvents
    .map(
      (ev) => {
        const score = ev.similarityScore ?? ev.similarity_score ?? 1.0;
        return `<div class="cross-file-row">
          <strong>${escHtml(ev.event.replace(/_/g, " "))}</strong>:
          ${escHtml(ev.description)}
          <span style="color:var(--vscode-descriptionForeground)">(${(score * 100).toFixed(0)}%)</span>
        </div>`;
      }
    )
    .join("");

  return `<div class="section">
    <h2>Cross-File Symbol Tracking (${symbolCount} symbols)</h2>
    ${rows}
  </div>`;
}

function renderPerformance(data: DiffOutput): string {
  const filesProc = getFilesProcessed(data);
  const nodesComp = getNodesCompared(data);
  const parseMs = getParseTimeMs(data);
  const diffMs = getDiffTimeMs(data);
  const totalMs = getTotalTimeMs(data);

  const p = data.performance;
  const incrParses = p ? (p.incrementalParses ?? p.incremental_parses) : undefined;
  const nodesReused = p ? (p.nodesReused ?? p.nodes_reused ?? 0) : 0;

  const items = [
    `${filesProc} files`,
    `${nodesComp} nodes`,
    `parse: ${parseMs.toFixed(1)}ms`,
    `diff: ${diffMs.toFixed(1)}ms`,
    `total: ${totalMs.toFixed(1)}ms`,
  ];
  if (incrParses) {
    items.push(`${incrParses} incremental, ${nodesReused} reused`);
  }
  return `<div class="perf-footer">${items.map((i) => `<span>${i}</span>`).join("")}</div>`;
}

function escHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

function escAttr(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/"/g, "&quot;");
}

function getNonce(): string {
  let text = "";
  const possible = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
  for (let i = 0; i < 32; i++) {
    text += possible.charAt(Math.floor(Math.random() * possible.length));
  }
  return text;
}
