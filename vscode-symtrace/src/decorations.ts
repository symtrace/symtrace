import * as vscode from "vscode";
import {
  DiffOutput,
  OperationType,
  getFilePath,
  getOpType,
  getEntityType,
  getOldLocation,
  getNewLocation,
  getSimilarityPercent,
  getChangeIntensity,
} from "./types";

const decorationTypes: Record<OperationType, vscode.TextEditorDecorationType> = {
  INSERT: vscode.window.createTextEditorDecorationType({
    overviewRulerColor: "#3fb950",
    overviewRulerLane: vscode.OverviewRulerLane.Left,
    gutterIconSize: "80%",
    after: {
      contentText: " [INSERTED]",
      color: "#3fb950",
      fontStyle: "italic",
      fontWeight: "normal",
    },
  }),
  DELETE: vscode.window.createTextEditorDecorationType({
    overviewRulerColor: "#f85149",
    overviewRulerLane: vscode.OverviewRulerLane.Left,
    after: {
      contentText: " [DELETED]",
      color: "#f85149",
      fontStyle: "italic",
      fontWeight: "normal",
    },
  }),
  MODIFY: vscode.window.createTextEditorDecorationType({
    overviewRulerColor: "#d29922",
    overviewRulerLane: vscode.OverviewRulerLane.Left,
    after: {
      contentText: " [MODIFIED]",
      color: "#d29922",
      fontStyle: "italic",
      fontWeight: "normal",
    },
  }),
  MOVE: vscode.window.createTextEditorDecorationType({
    overviewRulerColor: "#3794ff",
    overviewRulerLane: vscode.OverviewRulerLane.Left,
    after: {
      contentText: " [MOVED]",
      color: "#3794ff",
      fontStyle: "italic",
      fontWeight: "normal",
    },
  }),
  RENAME: vscode.window.createTextEditorDecorationType({
    overviewRulerColor: "#cca700",
    overviewRulerLane: vscode.OverviewRulerLane.Left,
    after: {
      contentText: " [RENAMED]",
      color: "#cca700",
      fontStyle: "italic",
      fontWeight: "normal",
    },
  }),
};

let currentData: DiffOutput | undefined;

export function applyDecorations(data: DiffOutput): void {
  currentData = data;
  for (const editor of vscode.window.visibleTextEditors) {
    decorateEditor(editor);
  }
}

export function clearDecorations(): void {
  currentData = undefined;
  for (const editor of vscode.window.visibleTextEditors) {
    for (const dt of Object.values(decorationTypes)) {
      editor.setDecorations(dt, []);
    }
  }
}

export function onEditorChange(editor: vscode.TextEditor | undefined): void {
  if (editor && currentData) {
    decorateEditor(editor);
  }
}

function decorateEditor(editor: vscode.TextEditor): void {
  if (!currentData) {
    return;
  }

  const relativePath = vscode.workspace.asRelativePath(editor.document.uri);
  const fileDiff = currentData.files.find((f) => {
    const pathStr = getFilePath(f);
    return pathStr === relativePath || pathStr === relativePath.replace(/\\/g, "/");
  });

  if (!fileDiff) {
    // Clear decorations for files not in the diff
    for (const dt of Object.values(decorationTypes)) {
      editor.setDecorations(dt, []);
    }
    return;
  }

  // Group operations by type
  const groups: Record<OperationType, vscode.DecorationOptions[]> = {
    INSERT: [],
    DELETE: [],
    MODIFY: [],
    MOVE: [],
    RENAME: [],
  };

  for (const op of fileDiff.operations) {
    const opType = getOpType(op);
    const entityType = getEntityType(op);
    const oldLoc = getOldLocation(op);
    const newLoc = getNewLocation(op);
    const simPercent = getSimilarityPercent(op.similarity);
    const intensity = getChangeIntensity(op.similarity);

    const loc = opType === "DELETE" ? oldLoc : newLoc;
    if (!loc) {
      continue;
    }
    const lineMatch = loc.match(/L(\d+)/);
    if (!lineMatch) {
      continue;
    }

    const line = parseInt(lineMatch[1], 10) - 1;
    if (line < 0 || line >= editor.document.lineCount) {
      continue;
    }

    const range = editor.document.lineAt(line).range;
    let hoverMessage = `**${opType}** ${entityType}: ${op.details}`;
    if (op.similarity) {
      hoverMessage += `\n\nSimilarity: ${simPercent.toFixed(0)}% (${intensity})`;
    }

    groups[opType].push({
      range,
      hoverMessage: new vscode.MarkdownString(hoverMessage),
    });
  }

  for (const [type, ranges] of Object.entries(groups)) {
    editor.setDecorations(decorationTypes[type as OperationType], ranges);
  }
}
