// TypeScript interfaces mirroring the symtrace Rust JSON output (src/types.rs)

export type OperationType = "MOVE" | "RENAME" | "INSERT" | "DELETE" | "MODIFY";
export type EntityType = "function" | "class" | "variable" | "block" | "other";
export type ChangeIntensity = "low" | "medium" | "high";
export type CommitClass = "refactor" | "feature" | "bug_fix" | "cleanup" | "formatting_only" | "mixed";
export type RefactorKind = "extract_method" | "move_method" | "rename_variable";
export type CrossFileEventKind = "cross_file_move" | "cross_file_rename" | "api_surface_change";

export interface SimilarityScore {
  structure_similarity?: number;
  structureSimilarity?: number;
  token_similarity?: number;
  tokenSimilarity?: number;
  node_count_delta?: number;
  nodeCountDelta?: number;
  cyclomatic_delta?: number;
  cyclomaticDelta?: number;
  control_flow_changed?: boolean;
  controlFlowChanged?: boolean;
  similarity_percent?: number;
  similarityPercent?: number;
  change_intensity?: ChangeIntensity;
  changeIntensity?: ChangeIntensity;
}

export interface OperationRecord {
  type?: OperationType;
  opType?: OperationType;
  entity_type?: EntityType;
  entityType?: EntityType;
  old_location?: string;
  oldLocation?: string;
  new_location?: string;
  newLocation?: string;
  details: string;
  similarity?: SimilarityScore;
}

export interface RefactorPattern {
  kind: RefactorKind;
  description: string;
  involved_entities?: string[];
  involvedEntities?: string[];
  confidence: number;
}

export interface FileDiff {
  file_path?: string;
  filePath?: string;
  operations: OperationRecord[];
  refactor_patterns?: RefactorPattern[];
  refactorPatterns?: RefactorPattern[];
}

export interface DiffSummary {
  total_files?: number;
  totalFiles?: number;
  moves: number;
  renames: number;
  inserts: number;
  deletes: number;
  modifications: number;
}

export interface CrossFileMatch {
  event: CrossFileEventKind;
  old_symbol?: string;
  oldSymbol?: string;
  old_file?: string;
  oldFile?: string;
  new_symbol?: string;
  newSymbol?: string;
  new_file?: string;
  newFile?: string;
  similarity_score?: number;
  similarityScore?: number;
  description: string;
}

export interface CrossFileTracking {
  symbol_count?: number;
  symbolCount?: number;
  cross_file_events?: CrossFileMatch[];
  crossFileEvents?: CrossFileMatch[];
}

export interface CommitClassification {
  primary_class?: CommitClass;
  primaryClass?: CommitClass;
  confidence_score?: number;
  confidenceScore?: number;
}

export interface PerformanceMetrics {
  total_files_processed?: number;
  totalFilesProcessed?: number;
  total_nodes_compared?: number;
  totalNodesCompared?: number;
  parse_time_ms?: number;
  parseTimeMs?: number;
  diff_time_ms?: number;
  diffTimeMs?: number;
  total_time_ms?: number;
  totalTimeMs?: number;
  incremental_parses?: number;
  incrementalParses?: number;
  nodes_reused?: number;
  nodesReused?: number;
}

export interface DiffOutput {
  repository: string;
  commit_a?: string;
  commitA?: string;
  commit_b?: string;
  commitB?: string;
  files: FileDiff[];
  summary: DiffSummary;
  cross_file_tracking?: CrossFileTracking;
  crossFileTracking?: CrossFileTracking;
  commit_classification?: CommitClassification;
  commitClassification?: CommitClassification;
  performance: PerformanceMetrics;
}

// Helper Accessors to handle both camelCase (CLI v0.4.5) and snake_case properties
export function getFilePath(file: FileDiff): string {
  return file.filePath || file.file_path || "";
}

export function getOpType(op: OperationRecord): OperationType {
  return op.opType || op.type || "MODIFY";
}

export function getEntityType(op: OperationRecord): EntityType {
  return op.entityType || op.entity_type || "other";
}

export function getOldLocation(op: OperationRecord): string | undefined {
  return op.oldLocation || op.old_location;
}

export function getNewLocation(op: OperationRecord): string | undefined {
  return op.newLocation || op.new_location;
}

export function getSimilarityPercent(score?: SimilarityScore): number {
  if (!score) return 0;
  return score.similarityPercent ?? score.similarity_percent ?? 0;
}

export function getChangeIntensity(score?: SimilarityScore): ChangeIntensity {
  if (!score) return "low";
  return score.changeIntensity || score.change_intensity || "low";
}

export function getCommitA(data: DiffOutput): string {
  return data.commitA || data.commit_a || "";
}

export function getCommitB(data: DiffOutput): string {
  return data.commitB || data.commit_b || "";
}

export function getTotalFiles(data: DiffOutput): number {
  return data.summary?.totalFiles ?? data.summary?.total_files ?? data.files.length;
}

export function getTotalTimeMs(data: DiffOutput): number {
  return data.performance?.totalTimeMs ?? data.performance?.total_time_ms ?? 0;
}

export function getParseTimeMs(data: DiffOutput): number {
  return data.performance?.parseTimeMs ?? data.performance?.parse_time_ms ?? 0;
}

export function getDiffTimeMs(data: DiffOutput): number {
  return data.performance?.diffTimeMs ?? data.performance?.diff_time_ms ?? 0;
}

export function getFilesProcessed(data: DiffOutput): number {
  return data.performance?.totalFilesProcessed ?? data.performance?.total_files_processed ?? data.files.length;
}

export function getNodesCompared(data: DiffOutput): number {
  return data.performance?.totalNodesCompared ?? data.performance?.total_nodes_compared ?? 0;
}

export function getPrimaryClass(data: DiffOutput): CommitClass {
  return data.commitClassification?.primaryClass || data.commit_classification?.primary_class || "refactor";
}

export function getConfidenceScore(data: DiffOutput): number {
  return data.commitClassification?.confidenceScore ?? data.commit_classification?.confidence_score ?? 1.0;
}

export function getCrossFileEvents(data: DiffOutput): CrossFileMatch[] {
  return data.crossFileTracking?.crossFileEvents || data.cross_file_tracking?.cross_file_events || [];
}

export function getRefactorPatterns(file: FileDiff): RefactorPattern[] {
  return file.refactorPatterns || file.refactor_patterns || [];
}

export function getInvolvedEntities(pattern: RefactorPattern): string[] {
  return pattern.involvedEntities || pattern.involved_entities || [];
}
