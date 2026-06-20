import { invoke } from "@tauri-apps/api/core";
import type { ScanOptions, ScanReport, ScanStatus } from "./types";

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
