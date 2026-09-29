export type ScanPhase = "idle" | "queued" | "running" | "completed" | "failed" | "cancelled";

export type ScanMode = "quick" | "deep";

export interface ScanOptions {
  drive: string;
  topCount: number;
  largeFileMb: number;
  scanMode: ScanMode;
}

export interface ScanStatus {
  scanId: string;
  phase: ScanPhase;
  percent: number;
  message: string;
  startedAt?: string;
  completedAt?: string;
  error?: string;
  markdownReportPath?: string;
  jsonReportPath?: string;
}

export interface PrivacyLedger {
  uploaded: boolean;
}

export type RecommendationCategory =
  "low-risk-cache" | "app-managed" | "user-data" | "uninstall-or-migrate" | "system-managed";

export type RecommendationRisk = "low" | "medium" | "high" | "blocked";

export type RecommendationSource = "scanner" | "dism" | "registry" | "heuristic";

/** 建议排序维度（Phase 3）。 */
export type RecommendationSort = "size" | "risk" | "confidence";

export interface Recommendation {
  id: string;
  path: string;
  sizeGb: number;
  category: RecommendationCategory;
  risk: RecommendationRisk;
  confidence: number;
  reason: string;
  manualSteps: string[];
  cleanable: boolean;
  blockedReason?: string;
  cleanupMethod?: string;
  requiresAppClosed: boolean;
  source: RecommendationSource;
}

/** 目录行视图（Phase 3 additive，透传自扫描内核 raw JSON 的 PS 属性语义）。 */
export interface DirRowView {
  path: string;
  sizeGb: number;
  files: number;
  dirs: number;
  skippedReparsePoints: number;
}

export interface DrilldownView {
  root: string;
  rows: DirRowView[];
}

export interface LargeFileView {
  path: string;
  sizeGb: number;
}

/** treemap 层级节点（只读聚合视图；residual 为截断/未列出体量的合并显示）。 */
export interface TreemapNodeDatum {
  id: string;
  name: string;
  sizeGb: number;
  kind: "root" | "dir" | "file" | "residual";
  children?: TreemapNodeDatum[];
}

export interface ScanReport {
  schemaVersion: string;
  scanId: string;
  createdAt: string;
  drive: string;
  isElevated: boolean;
  skippedReparsePoints: number;
  scanErrors: string[];
  privacy: PrivacyLedger;
  recommendations: Recommendation[];
  markdownReportPath: string;
  jsonReportPath: string;
  /** Phase 3 additive：原始顶层目录行（treemap/对比数据源；老报告缺省为空）。 */
  topRows: DirRowView[];
  drilldowns: DrilldownView[];
  largeFiles: LargeFileView[];
}

export interface ScanProgressEvent {
  scanId: string;
  phase: ScanPhase;
  percent: number;
  message: string;
}

/** 历史报告摘要（Phase 3 发现层：历史列表与对比入口，不展开建议明细）。 */
export interface ReportSummary {
  scanId: string;
  createdAt: string;
  drive: string;
  recommendationCount: number;
  totalSizeGb: number;
  scanErrorCount: number;
  isLatest: boolean;
}

// ==== Phase 5：实验性回收站清理（plan-first + action-log 审计） ====

/** 计划接受项：路径由后端从报告反查，前端只持有 id。 */
export interface CleanupPlanItem {
  id: string;
  path: string;
  sizeGb: number;
  category: RecommendationCategory;
  cleanupMethod?: string | null;
}

export interface CleanupPlanRejection {
  id: string;
  path: string;
  reason: string;
}

export interface CleanupPlan {
  reportId: string;
  items: CleanupPlanItem[];
  rejected: CleanupPlanRejection[];
  totalSizeGb: number;
  /** 后端保守预算（GB）：磁盘剩余的 1/10，超出的候选项会被拒绝。 */
  recycleBudgetGb: number;
}

export interface CleanupItemOutcome {
  id: string;
  path: string;
  sizeGb: number;
  status: "recycled" | "failed";
  message?: string | null;
}

export interface CleanupOutcome {
  reportId: string;
  results: CleanupItemOutcome[];
  rejected: CleanupPlanRejection[];
  recycledCount: number;
  failedCount: number;
  /** 成功回收项合计 GB。 */
  totalSizeGb: number;
  actionLogPath: string;
}

/** action-log 审计行（最新在前，上限 200 条）。 */
export interface ActionLogEntry {
  ts: string;
  reportId: string;
  id: string;
  path: string;
  sizeGb: number;
  action: string;
  status: string;
  message?: string | null;
}
