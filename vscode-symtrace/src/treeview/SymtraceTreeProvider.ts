import * as vscode from "vscode";
import {
  DiffOutput,
  getPrimaryClass,
  getConfidenceScore,
  getCommitA,
  getCommitB,
  getCrossFileEvents,
} from "../types";
import {
  SymtraceTreeItem,
  SummaryNode,
  FileNode,
  ClassificationNode,
  CrossFileSectionNode,
  PerformanceNode,
} from "./treeItems";

export class SymtraceTreeProvider
  implements vscode.TreeDataProvider<SymtraceTreeItem>
{
  private _onDidChangeTreeData = new vscode.EventEmitter<
    SymtraceTreeItem | undefined
  >();
  readonly onDidChangeTreeData = this._onDidChangeTreeData.event;

  private data: DiffOutput | undefined;
  private repoPath: string | undefined;

  setData(data: DiffOutput, repoPath: string): void {
    this.data = data;
    this.repoPath = repoPath;
    this._onDidChangeTreeData.fire(undefined);
  }

  refresh(): void {
    this._onDidChangeTreeData.fire(undefined);
  }

  clear(): void {
    this.data = undefined;
    this.repoPath = undefined;
    this._onDidChangeTreeData.fire(undefined);
  }

  getTreeItem(element: SymtraceTreeItem): vscode.TreeItem {
    return element;
  }

  getChildren(element?: SymtraceTreeItem): SymtraceTreeItem[] {
    if (!this.data) {
      return [];
    }

    if (!element) {
      return this.getRootChildren();
    }

    if ("getChildren" in element && typeof element.getChildren === "function") {
      return (element as { getChildren: () => SymtraceTreeItem[] }).getChildren();
    }

    return [];
  }

  private getRootChildren(): SymtraceTreeItem[] {
    const items: SymtraceTreeItem[] = [];
    const data = this.data!;
    const repoPath = this.repoPath!;

    // Commit classification badge
    const primaryClass = getPrimaryClass(data);
    const confidenceScore = getConfidenceScore(data);
    if (data.commitClassification || data.commit_classification) {
      items.push(
        new ClassificationNode(
          primaryClass,
          confidenceScore
        )
      );
    }

    // Summary
    items.push(new SummaryNode(data));

    // File nodes
    const commitAStr = getCommitA(data);
    const commitBStr = getCommitB(data);
    for (const file of data.files) {
      items.push(new FileNode(file, commitAStr, commitBStr, repoPath));
    }

    // Cross-file tracking
    const crossEvents = getCrossFileEvents(data);
    if (crossEvents.length > 0) {
      items.push(new CrossFileSectionNode(data.crossFileTracking || data.cross_file_tracking!));
    }

    // Performance metrics
    items.push(new PerformanceNode(data.performance));

    return items;
  }
}
