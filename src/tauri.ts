import { invoke } from "@tauri-apps/api/core";
import type {
  ActionLogEntry,
  CleanupOutcome,
  CleanupPlan,
  ReportSummary,
  ScanOptions,
  ScanReport,
  ScanStatus
} from "./types";

export function startScan(options: ScanOptions) {
  return invoke<string>("start_scan", { options });
}

export function getScanStatus(scanId: string) {
  return invoke<ScanStatus>("get_scan_status", { scanId });
}

export function cancelScan(scanId: string) {
  return invoke<ScanStatus>("cancel_scan", { scanId });
}

export function getScanReport(scanId: string) {
  return invoke<ScanReport>("get_scan_report", { scanId });
}

export function loadLatestReport() {
  return invoke<ScanReport | null>("load_latest_report");
}

export function listReports() {
  return invoke<ReportSummary[]>("list_reports");
}

export function loadReportById(scanId: string) {
  return invoke<ScanReport>("load_report_by_id", { scanId });
}

export function revealReport(scanId: string, kind: "markdown" | "json" | "folder") {
  return invoke<void>("reveal_report", { scanId, kind });
}

// ==== Phase 5：实验性回收站清理（路径由后端按 reportId 反查，前端只传候选 id） ====

export function planCleanup(reportId: string, candidateIds: string[]) {
  return invoke<CleanupPlan>("plan_cleanup", { reportId, candidateIds });
}

export function executeCleanup(reportId: string, candidateIds: string[]) {
  return invoke<CleanupOutcome>("execute_cleanup", { reportId, candidateIds });
}

export function getActionLog() {
  return invoke<ActionLogEntry[]>("get_action_log");
}
