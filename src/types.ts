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
  | "low-risk-cache"
  | "app-managed"
  | "user-data"
  | "uninstall-or-migrate"
  | "system-managed";

export type RecommendationRisk = "low" | "medium" | "high" | "blocked";

export type RecommendationSource = "scanner" | "dism" | "registry" | "heuristic";

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
}

export interface ScanProgressEvent {
  scanId: string;
  phase: ScanPhase;
  percent: number;
  message: string;
}
