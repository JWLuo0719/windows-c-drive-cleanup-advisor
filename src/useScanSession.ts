import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  cancelScan,
  executeCleanup as executeCleanupIpc,
  getScanReport,
  getScanStatus,
  loadLatestReport,
  loadReportById as fetchReportById,
  planCleanup as planCleanupIpc,
  revealReport,
  startScan
} from "./tauri";
import { scanStageDescription, SCAN_DEFAULTS } from "./scanStages";
import type {
  CleanupOutcome,
  CleanupPlan,
  Recommendation,
  ScanMode,
  ScanProgressEvent,
  ScanReport,
  ScanStatus
} from "./types";

export const initialStatus: ScanStatus = {
  scanId: "",
  phase: "idle",
  percent: 0,
  message: "准备进行只读扫描。"
};

const terminalPhases = new Set<ScanStatus["phase"]>(["completed", "failed", "cancelled"]);

export interface ScanActivityItem {
  id: string;
  time: string;
  message: string;
}

export interface ScanSession {
  status: ScanStatus;
  report: ScanReport | null;
  error: string | null;
  copyNotice: string | null;
  scanMode: ScanMode;
  selectedCategory: Recommendation["category"] | "all";
  scanStartedAt: number | null;
  lastProgressAt: number | null;
  clockTick: number;
  scanActivity: ScanActivityItem[];
  /** Phase 5：勾选中的清理候选 id（仅 low-risk-cache && cleanable 可勾）。 */
  selectedCandidateIds: string[];
  cleanupPlan: CleanupPlan | null;
  cleanupOutcome: CleanupOutcome | null;
  cleanupBusy: boolean;
  setScanMode: (mode: ScanMode) => void;
  setSelectedCategory: (category: Recommendation["category"] | "all") => void;
  toggleCandidate: (id: string) => void;
  planCleanup: () => Promise<void>;
  executeCleanup: () => Promise<void>;
  dismissCleanup: () => void;
  runScan: () => void;
  stopScan: () => void;
  loadRecentReport: () => void;
  loadReport: (scanId: string) => Promise<void>;
  showReport: (kind: "markdown" | "json" | "folder") => void;
  copyReportPaths: () => void;
}

function getErrorMessage(err: unknown) {
  return err instanceof Error ? err.message : String(err);
}

function formatActivityTime(value: number) {
  return new Intl.DateTimeFormat("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit"
  }).format(new Date(value));
}

/// 扫描会话状态机：进度、活动流、报告装载与报告操作的唯一所有者。
/// 会话跟踪的收尾（清轮询 + 清 activeScanId + 清计时）统一走 resetScanTracking，
/// 不允许在调用点手写这组重置（v0.2 曾在 8 处各写一遍四连重置，易漏易错）。
export function useScanSession(): ScanSession {
  const [status, setStatus] = useState<ScanStatus>(initialStatus);
  const [report, setReport] = useState<ScanReport | null>(null);
  const [scanMode, setScanMode] = useState<ScanMode>("quick");
  const [selectedCategory, setSelectedCategory] = useState<Recommendation["category"] | "all">(
    "all"
  );
  const [error, setError] = useState<string | null>(null);
  const [copyNotice, setCopyNotice] = useState<string | null>(null);
  const [scanStartedAt, setScanStartedAt] = useState<number | null>(null);
  const [lastProgressAt, setLastProgressAt] = useState<number | null>(null);
  const [clockTick, setClockTick] = useState(() => Date.now());
  const [scanActivity, setScanActivity] = useState<ScanActivityItem[]>([]);
  const [selectedCandidateIds, setSelectedCandidateIds] = useState<string[]>([]);
  const [cleanupPlan, setCleanupPlan] = useState<CleanupPlan | null>(null);
  const [cleanupOutcome, setCleanupOutcome] = useState<CleanupOutcome | null>(null);
  const [cleanupBusy, setCleanupBusy] = useState(false);
  const activeScanIdRef = useRef<string | null>(null);
  const pollTimerRef = useRef<number | null>(null);
  const statusRef = useRef<ScanStatus>(initialStatus);
  const lastStallActivityAtRef = useRef(0);

  function clearPollTimer() {
    if (pollTimerRef.current !== null) {
      window.clearInterval(pollTimerRef.current);
      pollTimerRef.current = null;
    }
  }

  /// 会话跟踪收尾四连的唯一入口：清轮询、注销活动扫描、清计时起点。
  function resetScanTracking() {
    clearPollTimer();
    activeScanIdRef.current = null;
    setScanStartedAt(null);
    setLastProgressAt(null);
  }

  /// 报告切换时清空清理上下文（勾选/计划/结果都属于旧报告，不能带到新报告上）。
  function resetCleanup() {
    setSelectedCandidateIds([]);
    setCleanupPlan(null);
    setCleanupOutcome(null);
  }

  function addScanActivity(message: string, now = Date.now()) {
    if (!message) {
      return;
    }
    setScanActivity((items) => {
      const last = items[0];
      if (last?.message === message) {
        return items;
      }
      return [
        {
          id: `${now}-${items.length}`,
          time: formatActivityTime(now),
          message
        },
        ...items
      ].slice(0, 5);
    });
  }

  function setTrackedStatus(nextStatus: ScanStatus) {
    const current = statusRef.current;
    const changed =
      current.scanId !== nextStatus.scanId ||
      current.phase !== nextStatus.phase ||
      current.percent !== nextStatus.percent ||
      current.message !== nextStatus.message;
    statusRef.current = nextStatus;
    if (changed) {
      const now = Date.now();
      setClockTick(now);
      setLastProgressAt(now);
      lastStallActivityAtRef.current = 0;
      if (nextStatus.message) {
        addScanActivity(nextStatus.message, now);
      }
    }
    setStatus(nextStatus);
  }

  useEffect(() => {
    statusRef.current = status;
  }, [status]);

  useEffect(() => {
    if (!scanStartedAt || !lastProgressAt || !activeScanIdRef.current) {
      return;
    }
    const timer = window.setInterval(() => {
      const now = Date.now();
      setClockTick(now);
      const current = statusRef.current;
      const stalledMs = now - lastProgressAt;
      const lastStallActivityMs = now - lastStallActivityAtRef.current;
      if (
        activeScanIdRef.current &&
        (current.phase === "queued" || current.phase === "running") &&
        stalledMs >= 30000 &&
        lastStallActivityMs >= 30000
      ) {
        lastStallActivityAtRef.current = now;
        addScanActivity(`当前阶段仍在工作：${scanStageDescription(current.percent)}`, now);
      }
    }, 1000);
    return () => window.clearInterval(timer);
  }, [scanStartedAt, lastProgressAt]);

  useEffect(() => {
    // listen() 清理竞态防护：effect 若在 listen 的 Promise 兑现前卸载，
    // unlisten 尚未赋值会导致监听器永久泄漏；disposed 标记让迟到的 dispose 立即执行。
    let disposed = false;
    let unlisten: (() => void) | undefined;
    listen<ScanProgressEvent>("scan-progress", (event) => {
      const progress = event.payload;
      if (!activeScanIdRef.current || activeScanIdRef.current !== progress.scanId) {
        return;
      }
      const current = statusRef.current;
      if (current.scanId && current.scanId !== progress.scanId) {
        return;
      }
      if (terminalPhases.has(current.phase) && !terminalPhases.has(progress.phase)) {
        return;
      }
      setTrackedStatus({
        ...current,
        scanId: progress.scanId,
        phase: progress.phase,
        percent: progress.percent,
        message: progress.message
      });
      if (progress.phase === "completed") {
        void loadCompletedReport(progress.scanId);
      }
    })
      .then((dispose) => {
        if (disposed) {
          dispose();
        } else {
          unlisten = dispose;
        }
      })
      .catch((err) => {
        setError(getErrorMessage(err));
      });
    return () => {
      disposed = true;
      unlisten?.();
      clearPollTimer();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- 挂载一次的事件订阅，与 v0.2 行为一致
  }, []);

  async function loadCompletedReport(scanId: string) {
    if (activeScanIdRef.current !== scanId) {
      return;
    }
    clearPollTimer();
    try {
      const nextReport = await getScanReport(scanId);
      if (activeScanIdRef.current !== scanId) {
        return;
      }
      resetScanTracking();
      setReport(nextReport);
      resetCleanup();
    } catch (err) {
      if (activeScanIdRef.current !== scanId) {
        return;
      }
      resetScanTracking();
      setError(getErrorMessage(err));
    }
  }

  async function runScan() {
    clearPollTimer();
    activeScanIdRef.current = null;
    setError(null);
    setCopyNotice(null);
    setReport(null);
    setSelectedCategory("all");
    setScanActivity([]);
    resetCleanup();
    lastStallActivityAtRef.current = 0;
    const now = Date.now();
    setScanStartedAt(now);
    setLastProgressAt(now);
    setClockTick(now);
    setTrackedStatus({
      scanId: "",
      phase: "queued",
      percent: 1,
      message: "正在提交只读扫描任务。"
    });

    try {
      const scanId = await startScan({
        drive: "C",
        topCount: SCAN_DEFAULTS.topCount,
        largeFileMb: SCAN_DEFAULTS.largeFileMb,
        scanMode
      });
      activeScanIdRef.current = scanId;
      setTrackedStatus({
        scanId,
        phase: "queued",
        percent: 2,
        message: "扫描已排队。本次不会执行任何清理动作。"
      });
      pollStatus(scanId);
    } catch (err) {
      resetScanTracking();
      setTrackedStatus(initialStatus);
      setError(getErrorMessage(err));
    }
  }

  function pollStatus(scanId: string) {
    clearPollTimer();
    const timer = window.setInterval(async () => {
      try {
        const nextStatus = await getScanStatus(scanId);
        if (activeScanIdRef.current !== scanId) {
          return;
        }
        setTrackedStatus(nextStatus);
        if (nextStatus.phase === "completed") {
          await loadCompletedReport(scanId);
        }
        if (["failed", "cancelled"].includes(nextStatus.phase)) {
          resetScanTracking();
          if (nextStatus.error) {
            setError(nextStatus.error);
          }
        }
      } catch (err) {
        if (activeScanIdRef.current !== scanId) {
          return;
        }
        resetScanTracking();
        setError(getErrorMessage(err));
      }
    }, 1200);
    pollTimerRef.current = timer;
  }

  async function stopScan() {
    if (!status.scanId) {
      return;
    }
    setError(null);
    try {
      const nextStatus = await cancelScan(status.scanId);
      resetScanTracking();
      setTrackedStatus(nextStatus);
    } catch (err) {
      setError(getErrorMessage(err));
    }
  }

  async function showReport(kind: "markdown" | "json" | "folder") {
    if (!report) {
      return;
    }
    setError(null);
    try {
      await revealReport(report.scanId, kind);
    } catch (err) {
      setError(getErrorMessage(err));
    }
  }

  async function copyReportPaths() {
    if (!report) {
      return;
    }
    const clipboard = navigator.clipboard;
    if (!clipboard) {
      setError("当前环境不支持直接复制，请打开报告目录后手动复制路径。");
      return;
    }
    setError(null);
    try {
      await clipboard.writeText(
        [`Markdown: ${report.markdownReportPath}`, `JSON: ${report.jsonReportPath}`].join("\n")
      );
      setCopyNotice("报告路径已复制。");
    } catch (err) {
      setError(getErrorMessage(err));
    }
  }

  async function loadRecentReport() {
    setError(null);
    setCopyNotice(null);
    try {
      const latestReport = await loadLatestReport();
      if (!latestReport) {
        setCopyNotice("没有找到最近的本地报告。");
        return;
      }
      resetScanTracking();
      setReport(latestReport);
      setSelectedCategory("all");
      setScanActivity([]);
      resetCleanup();
      setTrackedStatus({
        scanId: latestReport.scanId,
        phase: "completed",
        percent: 100,
        message: "已载入最近一次本地报告。本次没有执行任何清理动作。",
        completedAt: latestReport.createdAt,
        markdownReportPath: latestReport.markdownReportPath,
        jsonReportPath: latestReport.jsonReportPath
      });
      setCopyNotice("已载入最近一次本地报告。");
    } catch (err) {
      setError(getErrorMessage(err));
    }
  }

  /// 按 ID 载入历史报告（Phase 3 历史面板入口），收尾与 loadRecentReport 一致。
  async function loadReport(scanId: string) {
    setError(null);
    setCopyNotice(null);
    try {
      const nextReport = await fetchReportById(scanId);
      resetScanTracking();
      setReport(nextReport);
      setSelectedCategory("all");
      setScanActivity([]);
      resetCleanup();
      setTrackedStatus({
        scanId: nextReport.scanId,
        phase: "completed",
        percent: 100,
        message: "已载入选定历史报告。本次没有执行任何清理动作。",
        completedAt: nextReport.createdAt,
        markdownReportPath: nextReport.markdownReportPath,
        jsonReportPath: nextReport.jsonReportPath
      });
      setCopyNotice("已载入选定历史报告。");
    } catch (err) {
      setError(getErrorMessage(err));
    }
  }

  /// Phase 5：勾选切换。计划/结果一旦与勾选不一致就作废，避免展示过期面板。
  function toggleCandidate(id: string) {
    setSelectedCandidateIds((ids) =>
      ids.includes(id) ? ids.filter((item) => item !== id) : [...ids, id]
    );
    setCleanupPlan(null);
    setCleanupOutcome(null);
  }

  /// plan-first：先取后端 dry-run 计划（含拒绝明细），用户确认后才 executeCleanup。
  async function planCleanup() {
    if (!report || selectedCandidateIds.length === 0 || cleanupBusy) {
      return;
    }
    setError(null);
    setCleanupOutcome(null);
    setCleanupBusy(true);
    try {
      const plan = await planCleanupIpc(report.scanId, [...selectedCandidateIds]);
      setCleanupPlan(plan);
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setCleanupBusy(false);
    }
  }

  /// 执行清理：后端重新 build 计划并逐项删除时刻复判；这里只负责确认后的调用与结果落地。
  async function executeCleanup() {
    if (!report || selectedCandidateIds.length === 0 || cleanupBusy) {
      return;
    }
    setError(null);
    setCleanupBusy(true);
    try {
      const outcome = await executeCleanupIpc(report.scanId, [...selectedCandidateIds]);
      setCleanupOutcome(outcome);
      setCleanupPlan(null);
      setSelectedCandidateIds([]);
      setCopyNotice(
        `清理完成：回收 ${outcome.recycledCount} 项，失败 ${outcome.failedCount} 项。报告数据已过期，建议重新扫描。`
      );
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setCleanupBusy(false);
    }
  }

  function dismissCleanup() {
    setCleanupPlan(null);
    setCleanupOutcome(null);
  }

  return {
    status,
    report,
    error,
    copyNotice,
    scanMode,
    selectedCategory,
    scanStartedAt,
    lastProgressAt,
    clockTick,
    scanActivity,
    selectedCandidateIds,
    cleanupPlan,
    cleanupOutcome,
    cleanupBusy,
    setScanMode,
    setSelectedCategory,
    toggleCandidate,
    planCleanup,
    executeCleanup,
    dismissCleanup,
    runScan,
    stopScan,
    loadRecentReport,
    loadReport,
    showReport,
    copyReportPaths
  };
}
