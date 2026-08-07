import * as vscode from "vscode";
import {
  DiffOutput,
  FileDiff,
  OperationRecord,
  OperationType,
  CrossFileTracking,
  CrossFileMatch,
  RefactorPattern,
  PerformanceMetrics,
  getFilePath,
  getOpType,
  getEntityType,
  getOldLocation,
  getNewLocation,
  getSimilarityPercent,
  getChangeIntensity,
  getInvolvedEntities,
  getRefactorPatterns,
  getFilesProcessed,
  getNodesCompared,
  getParseTimeMs,
  getDiffTimeMs,
  getTotalTimeMs,
  getTotalFiles,
} from "../types";

export type SymtraceTreeItem =
  | SummaryNode
  | SummaryDetailNode
  | FileNode
  | OperationNode
  | CrossFileSectionNode
  | CrossFileEventNode
  | ClassificationNode
  | RefactorPatternNode
  | PerformanceNode;

export class SummaryNode extends vscode.TreeItem {
  constructor(private data: DiffOutput) {
    super("Summary", vscode.TreeItemCollapsibleState.Expanded);
    this.iconPath = new vscode.ThemeIcon("dashboard");
    this.contextValue = "summary";
  }

  getChildren(): SummaryDetailNode[] {
    const s = this.data.summary;
    return [
      new SummaryDetailNode("Files", getTotalFiles(this.data), "files"),
      new SummaryDetailNode("Inserts", s.inserts, "diff-added"),
      new SummaryDetailNode("Deletes", s.deletes, "diff-removed"),
      new SummaryDetailNode("Modifications", s.modifications, "diff-modified"),
      new SummaryDetailNode("Moves", s.moves, "arrow-both"),
      new SummaryDetailNode("Renames", s.renames, "diff-renamed"),
    ];
  }
}

export class SummaryDetailNode extends vscode.TreeItem {
  constructor(label: string, value: number | string, icon: string) {
    super(`${label}: ${value}`, vscode.TreeItemCollapsibleState.None);
    this.iconPath = new vscode.ThemeIcon(icon);
  }
}

export class FileNode extends vscode.TreeItem {
  constructor(
    private fileDiff: FileDiff,
    private readonly commitA: string,
    private readonly commitB: string,
    private readonly repoPath: string
  ) {
    const pathStr = getFilePath(fileDiff);
    super(pathStr, vscode.TreeItemCollapsibleState.Expanded);
    const opCount = fileDiff.operations.length;
    this.description = `${opCount} operation${opCount !== 1 ? "s" : ""}`;
    this.iconPath = new vscode.ThemeIcon("file-code");
    this.contextValue = "file";
  }

  getChildren(): (OperationNode | RefactorPatternNode)[] {
    const filePathStr = getFilePath(this.fileDiff);
    const ops = this.fileDiff.operations.map(
      (op) =>
        new OperationNode(
          op,
          filePathStr,
          this.commitA,
          this.commitB,
          this.repoPath
        )
    );
    const refactors = getRefactorPatterns(this.fileDiff).map(
      (r) => new RefactorPatternNode(r)
    );
    return [...ops, ...refactors];
  }
}

function getOperationIcon(type: OperationType): vscode.ThemeIcon {
  switch (type) {
    case "INSERT":
      return new vscode.ThemeIcon(
        "diff-added",
        new vscode.ThemeColor("gitDecoration.addedResourceForeground")
      );
    case "DELETE":
      return new vscode.ThemeIcon(
        "diff-removed",
        new vscode.ThemeColor("gitDecoration.deletedResourceForeground")
      );
    case "MODIFY":
      return new vscode.ThemeIcon(
        "diff-modified",
        new vscode.ThemeColor("gitDecoration.modifiedResourceForeground")
      );
    case "MOVE":
      return new vscode.ThemeIcon(
        "arrow-both",
        new vscode.ThemeColor("editorInfo.foreground")
      );
    case "RENAME":
      return new vscode.ThemeIcon(
        "diff-renamed",
        new vscode.ThemeColor("editorWarning.foreground")
      );
  }
}

export class OperationNode extends vscode.TreeItem {
  constructor(
    public readonly operation: OperationRecord,
    public readonly filePath: string,
    private readonly commitA: string,
    private readonly commitB: string,
    private readonly repoPath: string
  ) {
    const opType = getOpType(operation);
    const entityType = getEntityType(operation);
    const oldLoc = getOldLocation(operation);
    const newLoc = getNewLocation(operation);
    const simPercent = getSimilarityPercent(operation.similarity);
    const intensity = getChangeIntensity(operation.similarity);

    super(
      `${opType} ${entityType}`,
      vscode.TreeItemCollapsibleState.None
    );

    this.iconPath = getOperationIcon(opType);
    this.description = operation.details;

    const loc = newLoc ?? oldLoc ?? "";
    let tooltip = `${opType} ${entityType}: ${operation.details}`;
    if (loc) {
      tooltip += `\nLocation: ${loc}`;
    }
    if (operation.similarity) {
      tooltip += `\nSimilarity: ${simPercent.toFixed(0)}% (${intensity})`;
    }
    tooltip += `\n\nClick to open side-by-side diff`;
    this.tooltip = tooltip;

    // Click to open side-by-side diff view
    this.command = {
      command: "symtrace.showOperationDiff",
      title: "Show Diff",
      arguments: [filePath, commitA, commitB, repoPath],
    };
  }
}

export class ClassificationNode extends vscode.TreeItem {
  constructor(primaryClass: string, confidence: number) {
    super(
      `Commit: ${primaryClass}`,
      vscode.TreeItemCollapsibleState.None
    );
    this.description = `${(confidence * 100).toFixed(0)}% confidence`;
    this.iconPath = new vscode.ThemeIcon("tag");
    this.contextValue = "classification";
  }
}

export class RefactorPatternNode extends vscode.TreeItem {
  constructor(pattern: RefactorPattern) {
    const kindLabel = pattern.kind.replace(/_/g, " ");
    super(kindLabel, vscode.TreeItemCollapsibleState.None);
    this.description = `${pattern.description} (${(pattern.confidence * 100).toFixed(0)}%)`;
    this.iconPath = new vscode.ThemeIcon(
      "wand",
      new vscode.ThemeColor("charts.purple")
    );
    const entities = getInvolvedEntities(pattern).join(", ");
    this.tooltip = `Refactor: ${kindLabel}\n${pattern.description}\nEntities: ${entities}\nConfidence: ${(pattern.confidence * 100).toFixed(0)}%`;
    this.contextValue = "refactorPattern";
  }
}

export class CrossFileSectionNode extends vscode.TreeItem {
  constructor(private tracking: CrossFileTracking) {
    super("Cross-File Events", vscode.TreeItemCollapsibleState.Collapsed);
    const events = tracking.crossFileEvents || tracking.cross_file_events || [];
    this.description = `${events.length} event${events.length !== 1 ? "s" : ""}`;
    this.iconPath = new vscode.ThemeIcon("references");
    this.contextValue = "crossFileSection";
  }

  getChildren(): CrossFileEventNode[] {
    const events = this.tracking.crossFileEvents || this.tracking.cross_file_events || [];
    return events.map((ev) => new CrossFileEventNode(ev));
  }
}

export class CrossFileEventNode extends vscode.TreeItem {
  constructor(event: CrossFileMatch) {
    const oldSym = event.oldSymbol || event.old_symbol || "";
    const oldF = event.oldFile || event.old_file || "";
    const newSym = event.newSymbol || event.new_symbol || "";
    const newF = event.newFile || event.new_file || "";
    const score = event.similarityScore ?? event.similarity_score ?? 1.0;

    const label =
      event.event === "cross_file_move"
        ? "Move"
        : event.event === "cross_file_rename"
          ? "Rename"
          : "API Change";
    super(label, vscode.TreeItemCollapsibleState.None);
    this.description = event.description;
    this.tooltip = `${oldSym} (${oldF}) -> ${newSym} (${newF})\nSimilarity: ${(score * 100).toFixed(0)}%`;

    const iconMap: Record<string, string> = {
      cross_file_move: "arrow-both",
      cross_file_rename: "diff-renamed",
      api_surface_change: "warning",
    };
    this.iconPath = new vscode.ThemeIcon(iconMap[event.event] ?? "circle");
  }
}

export class PerformanceNode extends vscode.TreeItem {
  constructor(private perf: PerformanceMetrics) {
    super("Performance", vscode.TreeItemCollapsibleState.Collapsed);
    const totalMs = perf.totalTimeMs ?? perf.total_time_ms ?? 0;
    this.description = `${totalMs.toFixed(1)}ms total`;
    this.iconPath = new vscode.ThemeIcon("dashboard");
    this.contextValue = "performance";
  }

  getChildren(): SummaryDetailNode[] {
    const filesProc = this.perf.totalFilesProcessed ?? this.perf.total_files_processed ?? 0;
    const nodesComp = this.perf.totalNodesCompared ?? this.perf.total_nodes_compared ?? 0;
    const parseMs = this.perf.parseTimeMs ?? this.perf.parse_time_ms ?? 0;
    const diffMs = this.perf.diffTimeMs ?? this.perf.diff_time_ms ?? 0;
    const totalMs = this.perf.totalTimeMs ?? this.perf.total_time_ms ?? 0;
    const incrParses = this.perf.incrementalParses ?? this.perf.incremental_parses;
    const nodesReused = this.perf.nodesReused ?? this.perf.nodes_reused;

    const items: SummaryDetailNode[] = [
      new SummaryDetailNode(
        "Files processed",
        filesProc,
        "files"
      ),
      new SummaryDetailNode(
        "Nodes compared",
        nodesComp,
        "symbol-number"
      ),
      new SummaryDetailNode(
        "Parse time",
        `${parseMs.toFixed(1)}ms`,
        "clock"
      ),
      new SummaryDetailNode(
        "Diff time",
        `${diffMs.toFixed(1)}ms`,
        "clock"
      ),
      new SummaryDetailNode(
        "Total time",
        `${totalMs.toFixed(1)}ms`,
        "clock"
      ),
    ];
    if (incrParses != null) {
      items.push(
        new SummaryDetailNode(
          "Incremental parses",
          incrParses,
          "sync"
        )
      );
    }
    if (nodesReused != null) {
      items.push(
        new SummaryDetailNode("Nodes reused", nodesReused, "sync")
      );
    }
    return items;
  }
}
