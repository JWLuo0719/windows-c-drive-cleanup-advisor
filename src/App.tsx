import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Activity,
  Ban,
  CheckCircle2,
  ClipboardCopy,
  Clock3,
  FileJson,
  FileText,
  FolderOpen,
  HardDrive,
  History,
  Loader2,
  PauseCircle,
  Play,
  ShieldCheck,
  Sparkles,
  XCircle
} from "lucide-react";
import {
  categoryLabels,
  buildReportHealthChecks,
  estimatePriorityReviewSize,
  filterRecommendations,
  formatSize,
  groupRecommendationTotals,
  riskLabels,
  sourceLabels,
  sortRecommendationsForReview,
  summarizeScanErrors
} from "./reportUtils";
import { cancelScan, getScanReport, getScanStatus, loadLatestReport, revealReport, startScan } from "./tauri";
import type { Recommendation, ScanMode, ScanProgressEvent, ScanReport, ScanStatus } from "./types";

const initialStatus: ScanStatus = {
  scanId: "",
  phase: "idle",
  percent: 0,
  message: "准备进行只读扫描。"
};

const terminalPhases = new Set<ScanStatus["phase"]>(["completed", "failed", "cancelled"]);

const scanTips = [
  "进度不动通常表示当前目录仍在枚举，应用没有卡死。",
  "受保护目录读取失败是正常现象，扫描器会记录后继续。",
  "扫描只会读取和生成报告，不会删除、移动或上传文件。",
  "低风险缓存也需要先关闭相关应用，再人工复核。",
  "系统托管目录只给出提示，不会变成清理任务。"
];

interface ScanActivityItem {
  id: string;
  time: string;
  message: string;
}

const scanProfiles: Record<ScanMode, { label: string; detail: string; topCount: number; largeFileMb: number }> = {
  quick: {
    label: "快速扫描",
    detail: "约数分钟，跳过重复深挖，适合日常检查",
    topCount: 30,
    largeFileMb: 200
  },
  deep: {
    label: "完整扫描",
    detail: "会额外深挖常见目录，结果更细但耗时更久",
    topCount: 30,
    largeFileMb: 200
  }
};

function statusTone(phase: ScanStatus["phase"]) {
  if (phase === "failed") {
    return "bad";
  }
  if (phase === "completed") {
    return "good";
  }
  if (phase === "cancelled") {
    return "muted";
  }
  return "work";
}

function formatCreatedAt(value?: string) {
  if (!value) {
    return "尚未生成";
  }
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return new Intl.DateTimeFormat("zh-CN", {
    dateStyle: "medium",
    timeStyle: "short"
  }).format(date);
}

function getErrorMessage(err: unknown) {
  return err instanceof Error ? err.message : String(err);
}

function formatDuration(ms: number) {
  const safeSeconds = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(safeSeconds / 60);
  const seconds = safeSeconds % 60;
  if (minutes === 0) {
    return `${seconds} 秒`;
  }
  return `${minutes} 分 ${seconds.toString().padStart(2, "0")} 秒`;
}

function scanStageDescription(percent: number) {
  if (percent < 12) {
    return "正在准备扫描任务和报告目录。";
  }
  if (percent < 18) {
    return "正在读取 C 盘基础容量信息。";
  }
  if (percent < 65) {
    return "正在统计 C 盘顶层真实目录，这一步遇到大目录时会停留较久。";
  }
  if (percent < 74) {
    return "正在查找大文件，文件数量多时进度可能暂时不变。";
  }
  if (percent < 88) {
    return "正在读取系统状态并整理安全边界。";
  }
  return "正在写入本地 Markdown 和 JSON 报告。";
}

function scanStageInsight(percent: number, mode: ScanMode, message: string) {
  const isDeepDrilldown = mode === "deep" && (percent >= 35 || message.includes("重点目录"));

  if (percent < 12) {
    return {
      focus: "提交任务",
      reason: "正在建立只读扫描队列和本地报告位置。",
      next: "读取磁盘容量"
    };
  }
  if (percent < 18) {
    return {
      focus: "读取容量",
      reason: "需要确认 C 盘空间、权限和基础环境。",
      next: "统计顶层目录"
    };
  }
  if (percent < 65) {
    if (isDeepDrilldown) {
      return {
        focus: "深挖重点目录",
        reason: "完整扫描会额外统计常见用户目录和 AppData。",
        next: "查找大文件"
      };
    }
    return {
      focus: "统计顶层目录",
      reason: "正在逐个计算真实目录体量，并跳过链接和受保护位置。",
      next: mode === "deep" ? "深挖重点目录" : "查找大文件"
    };
  }
  if (percent < 74) {
    return {
      focus: "查找大文件",
      reason: "文件数量多时需要枚举候选路径，进度会小步推进。",
      next: "读取系统状态"
    };
  }
  if (percent < 88) {
    return {
      focus: "读取系统状态",
      reason: "正在收集 pagefile、休眠和系统托管项的只读信息。",
      next: "生成报告"
    };
  }
  return {
    focus: "生成报告",
    reason: "正在把扫描结果写成本地 Markdown 和 JSON。",
    next: "展示结果"
  };
}

function formatActivityTime(value: number) {
  return new Intl.DateTimeFormat("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit"
  }).format(new Date(value));
}

export function App() {
  const [status, setStatus] = useState<ScanStatus>(initialStatus);
  const [report, setReport] = useState<ScanReport | null>(null);
  const [selectedCategory, setSelectedCategory] = useState<Recommendation["category"] | "all">("all");
  const [scanMode, setScanMode] = useState<ScanMode>("quick");
  const [error, setError] = useState<string | null>(null);
  const [copyNotice, setCopyNotice] = useState<string | null>(null);
  const [scanStartedAt, setScanStartedAt] = useState<number | null>(null);
  const [lastProgressAt, setLastProgressAt] = useState<number | null>(null);
  const [clockTick, setClockTick] = useState(() => Date.now());
  const [scanActivity, setScanActivity] = useState<ScanActivityItem[]>([]);
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

  function addScanActivity(message: string, now = Date.now()) {
    if (!message) {
      return;
    }
    setScanActivity((items) => {
      const last = items[0];
      if (last?.message === message) {
        return items;
      }
      return [{
        id: `${now}-${items.length}`,
        time: formatActivityTime(now),
        message
      }, ...items].slice(0, 5);
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
    }).then((dispose) => {
      unlisten = dispose;
    }).catch((err) => {
      setError(getErrorMessage(err));
    });
    return () => {
      unlisten?.();
      clearPollTimer();
    };
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
      activeScanIdRef.current = null;
      setScanStartedAt(null);
      setLastProgressAt(null);
      setReport(nextReport);
    } catch (err) {
      if (activeScanIdRef.current !== scanId) {
        return;
      }
      activeScanIdRef.current = null;
      setScanStartedAt(null);
      setLastProgressAt(null);
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
      const profile = scanProfiles[scanMode];
      const scanId = await startScan({
        drive: "C",
        topCount: profile.topCount,
        largeFileMb: profile.largeFileMb,
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
      activeScanIdRef.current = null;
      setScanStartedAt(null);
      setLastProgressAt(null);
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
          clearPollTimer();
          activeScanIdRef.current = null;
          setScanStartedAt(null);
          setLastProgressAt(null);
          if (nextStatus.error) {
            setError(nextStatus.error);
          }
        }
      } catch (err) {
        if (activeScanIdRef.current !== scanId) {
          return;
        }
        clearPollTimer();
        activeScanIdRef.current = null;
        setScanStartedAt(null);
        setLastProgressAt(null);
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
      clearPollTimer();
      activeScanIdRef.current = null;
      setScanStartedAt(null);
      setLastProgressAt(null);
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
      await clipboard.writeText([
        `Markdown: ${report.markdownReportPath}`,
        `JSON: ${report.jsonReportPath}`
      ].join("\n"));
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
      clearPollTimer();
      activeScanIdRef.current = null;
      setScanStartedAt(null);
      setLastProgressAt(null);
      setReport(latestReport);
      setSelectedCategory("all");
      setScanActivity([]);
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

  const filteredRecommendations = useMemo(() => {
    return sortRecommendationsForReview(
      filterRecommendations(report?.recommendations ?? [], selectedCategory),
      selectedCategory
    );
  }, [report, selectedCategory]);

  const groupedTotals = useMemo(() => {
    return groupRecommendationTotals(report?.recommendations ?? []);
  }, [report]);

  const totalReviewSize = useMemo(() => {
    return estimatePriorityReviewSize(report?.recommendations ?? []);
  }, [report]);

  const scanErrorSummary = useMemo(() => {
    return summarizeScanErrors(report?.scanErrors ?? []);
  }, [report]);

  const reportHealthChecks = useMemo(() => {
    return report ? buildReportHealthChecks(report) : [];
  }, [report]);

  const isWorking = status.phase === "queued" || status.phase === "running";
  const safePercent = Math.max(0, Math.min(100, status.percent));
  const selectedCategoryName = selectedCategory === "all" ? "全部建议" : categoryLabels[selectedCategory];
  const selectedProfile = scanProfiles[scanMode];
  const elapsedMs = scanStartedAt ? clockTick - scanStartedAt : 0;
  const currentStageMs = lastProgressAt ? clockTick - lastProgressAt : 0;
  const tipIndex = Math.floor(Math.max(0, elapsedMs) / 15000) % scanTips.length;
  const isProgressStalled = isWorking && currentStageMs >= 15000;
  const stageInsight = scanStageInsight(safePercent, scanMode, status.message);

  return (
    <main className="app-shell">
      <section className="command-band">
        <div className="command-copy">
          <p className="eyebrow">Windows C 盘清理顾问</p>
          <h1>C 盘空间诊断，先看清楚，再决定。</h1>
          <p className="intro">
            这是一套只读检查台：扫描真实本地目录、跳过重解析点、生成本机报告，并把清理建议交给你人工判断。
          </p>
          <div className="mode-selector" aria-label="扫描模式">
            {(Object.keys(scanProfiles) as ScanMode[]).map((mode) => (
              <button
                className={scanMode === mode ? "selected" : ""}
                key={mode}
                type="button"
                onClick={() => setScanMode(mode)}
                disabled={isWorking}
              >
                <strong>{scanProfiles[mode].label}</strong>
                <span>{scanProfiles[mode].detail}</span>
              </button>
            ))}
          </div>
        </div>
        <div className="command-actions" aria-label="扫描操作">
          <button className="primary-action" type="button" onClick={runScan} disabled={isWorking}>
            {isWorking ? <Loader2 className="spin" size={18} /> : <Play size={18} />}
            开始扫描 C 盘
          </button>
          <button className="icon-action" type="button" onClick={stopScan} disabled={!isWorking} title="取消扫描">
            <PauseCircle size={19} />
          </button>
        </div>
      </section>

      <section className="safety-ledger" aria-label="安全账本">
        <div>
          <ShieldCheck size={19} />
          <strong>零清理动作</strong>
          <span>v0.2 只读扫描</span>
        </div>
        <div>
          <Ban size={19} />
          <strong>零数据上传</strong>
          <span>报告留在本机</span>
        </div>
        <div>
          <FolderOpen size={19} />
          <strong>跳过虚拟目录</strong>
          <span>{report ? `${report.skippedReparsePoints} 个重解析点` : "扫描后显示数量"}</span>
        </div>
        <div>
          <HardDrive size={19} />
          <strong>系统项不自动处理</strong>
          <span>WinSxS / Installer 等仅提示</span>
        </div>
        <div>
          <FileText size={19} />
          <strong>记录受限路径</strong>
          <span>{report ? `${scanErrorSummary.count} 条读取受限` : "不会中断扫描"}</span>
        </div>
      </section>

      <section className="scan-strip">
        <div className={`status-orb ${statusTone(status.phase)}`}>
          {status.phase === "completed" ? <CheckCircle2 size={25} /> : <HardDrive size={25} />}
        </div>
        <div className="status-copy">
          <strong>{status.message}</strong>
          <span>{status.scanId ? `扫描 ID：${status.scanId}` : `${selectedProfile.label}：${selectedProfile.detail}`}</span>
        </div>
        <div className="progress-block" aria-label="扫描进度">
          <div className="progress-track">
            <div style={{ width: `${safePercent}%` }} />
          </div>
          <span>{safePercent}%</span>
        </div>
      </section>

      {isWorking ? (
        <section className="scan-companion" aria-label="扫描陪伴">
          <div className="companion-mark" aria-hidden="true">
            <Activity size={22} />
          </div>
          <div className="companion-copy">
            <strong>{isProgressStalled ? "当前阶段仍在工作" : "扫描正在进行"}</strong>
            <span>{scanStageDescription(safePercent)}</span>
            <p>{scanTips[tipIndex]}</p>
          </div>
          <div className="companion-metrics">
            <div>
              <Clock3 size={15} />
              <span>已用时 {formatDuration(elapsedMs)}</span>
            </div>
            <div>
              <Activity size={15} />
              <span>当前阶段 {formatDuration(currentStageMs)}</span>
            </div>
          </div>
          <div className="companion-stage" aria-label="阶段说明">
            <div>
              <span>当前焦点</span>
              <strong>{stageInsight.focus}</strong>
            </div>
            <div>
              <span>为什么慢</span>
              <strong>{stageInsight.reason}</strong>
            </div>
            <div>
              <span>下一步</span>
              <strong>{stageInsight.next}</strong>
            </div>
          </div>
          <div className="activity-feed" role="region" aria-label="扫描活动">
            {scanActivity.map((item) => (
              <div className="activity-feed-item" key={item.id}>
                <time>{item.time}</time>
                <span>{item.message}</span>
              </div>
            ))}
          </div>
        </section>
      ) : null}

      {error ? (
        <section className="error-band">
          <XCircle size={20} />
          <span>{error}</span>
        </section>
      ) : null}

      <section className="workspace-grid">
        <aside className="summary-pane">
          <section className="stat-board" aria-label="报告摘要">
            <div>
              <span>优先复核体量</span>
              <strong>{formatSize(totalReviewSize)}</strong>
            </div>
            <div>
              <span>建议数量</span>
              <strong>{report?.recommendations.length ?? 0}</strong>
            </div>
            <div>
              <span>报告时间</span>
              <strong>{formatCreatedAt(report?.createdAt)}</strong>
            </div>
            <div>
              <span>管理员权限</span>
              <strong>{report ? (report.isElevated ? "已提升" : "普通用户") : "待检测"}</strong>
            </div>
          </section>

          <section className="filter-panel" aria-label="风险分组">
            <h2>风险分组</h2>
            <button
              className={selectedCategory === "all" ? "filter-row selected" : "filter-row"}
              type="button"
              onClick={() => setSelectedCategory("all")}
            >
              <span>全部建议</span>
              <strong>{report?.recommendations.length ?? 0}</strong>
            </button>
            {(Object.keys(categoryLabels) as Recommendation["category"][]).map((category) => (
              <button
                className={selectedCategory === category ? "filter-row selected" : "filter-row"}
                key={category}
                type="button"
                onClick={() => setSelectedCategory(category)}
              >
                <span>{categoryLabels[category]}</span>
                <strong>{formatSize(groupedTotals.get(category) ?? 0)}</strong>
              </button>
            ))}
          </section>

          <section className="scan-notes" aria-label="扫描备注">
            <h2>扫描备注</h2>
            {!report ? (
              <p>扫描完成后，这里会显示跳过项、权限受限路径和报告位置。</p>
            ) : scanErrorSummary.count === 0 ? (
              <p>本次扫描没有记录读取失败。完整结构化信息可查看 JSON 报告。</p>
            ) : (
              <>
                <p>有 {scanErrorSummary.count} 条路径因权限或系统保护无法读取，扫描已继续完成。</p>
                <ul className="scan-error-buckets">
                  {scanErrorSummary.buckets.map((bucket) => (
                    <li key={bucket.id}>
                      <strong>{bucket.label}</strong>
                      <span>{bucket.count} 条</span>
                      <small>{bucket.examples[0]}</small>
                    </li>
                  ))}
                </ul>
                {scanErrorSummary.hasMore ? <p>完整受限路径已写入本地 JSON 报告。</p> : null}
              </>
            )}
          </section>

          <section className="health-panel" aria-label="结果自检">
            <h2>结果自检</h2>
            {!report ? (
              <p>扫描完成后，这里会自动检查报告隐私、建议数量和安全分类。</p>
            ) : (
              <div className="health-list">
                {reportHealthChecks.map((item) => (
                  <div className={`health-item ${item.tone}`} key={item.id}>
                    <strong>{item.label}</strong>
                    <span>{item.detail}</span>
                  </div>
                ))}
              </div>
            )}
          </section>

          <section className="result-guide" aria-label="结果判读">
            <h2>结果判读</h2>
            <ul>
              <li>
                <strong>先看低风险缓存</strong>
                <span>关闭相关应用后，用应用内清理或手动复核缓存目录。</span>
              </li>
              <li>
                <strong>系统托管项只提示</strong>
                <span>WinSxS、Installer、pagefile、Recovery 不要直接删除。</span>
              </li>
              <li>
                <strong>受限路径不等于失败</strong>
                <span>普通权限下 WindowsApps、Defender、回收站等拒绝访问是正常现象。</span>
              </li>
              <li>
                <strong>虚拟/链接目录已跳过</strong>
                <span>重解析点不会被当作真实 C 盘占用重复计算。</span>
              </li>
            </ul>
          </section>

          <section className="report-links" aria-label="本地报告">
            <h2>本地报告</h2>
            <p>{report ? "生成的文件只保存在这台电脑上。" : "运行扫描后会生成 Markdown 和 JSON 报告，也可以载入最近一次本地报告。"}</p>
            <div className="path-pill">
              <FileText size={16} />
              <span>{report?.markdownReportPath ?? "Markdown 报告待生成"}</span>
            </div>
            <div className="path-pill">
              <FileJson size={16} />
              <span>{report?.jsonReportPath ?? "JSON 报告待生成"}</span>
            </div>
            <div className="report-actions">
              <button type="button" onClick={loadRecentReport} disabled={isWorking}>
                <History size={16} />
                载入最近报告
              </button>
              <button type="button" onClick={() => showReport("markdown")} disabled={!report}>
                <FileText size={16} />
                显示 Markdown
              </button>
              <button type="button" onClick={() => showReport("json")} disabled={!report}>
                <FileJson size={16} />
                显示 JSON
              </button>
              <button type="button" onClick={() => showReport("folder")} disabled={!report}>
                <FolderOpen size={16} />
                打开报告目录
              </button>
              <button type="button" onClick={copyReportPaths} disabled={!report}>
                <ClipboardCopy size={16} />
                复制报告路径
              </button>
            </div>
            {copyNotice ? <p className="copy-notice">{copyNotice}</p> : null}
          </section>
        </aside>

        <section className="recommendation-pane">
          <div className="pane-heading">
            <div>
              <h2>空间建议</h2>
              <p>v0.2 只提供判断依据和手动步骤，清理能力会在后续版本单独上线。</p>
            </div>
            <div className="readonly-badge">
              <Sparkles size={16} />
              v0.2 只读
            </div>
          </div>

          {!report ? (
            <div className="empty-state">
              <Activity size={28} />
              <strong>还没有加载报告。</strong>
              <span>开始扫描后，这里会展示大文件夹、缓存候选和系统禁止项。</span>
            </div>
          ) : filteredRecommendations.length === 0 ? (
            <div className="empty-state">
              <Activity size={28} />
              <strong>当前分组没有建议。</strong>
              <span>{selectedCategoryName} 暂无可展示项目，完整结果仍可在本地报告中查看。</span>
            </div>
          ) : (
            <div className="recommendation-list">
              {filteredRecommendations.map((item) => (
                <article className={`recommendation ${item.risk}`} key={item.id}>
                  <div className="recommendation-topline">
                    <span className="size">{formatSize(item.sizeGb)}</span>
                    <span className="category">{categoryLabels[item.category]}</span>
                    <span className={`risk ${item.risk}`}>{riskLabels[item.risk]}</span>
                  </div>
                  <h3>{item.path}</h3>
                  <p>{item.reason}</p>
                  <div className="manual-steps">
                    {item.manualSteps.map((step) => (
                      <span key={step}>{step}</span>
                    ))}
                  </div>
                  <div className="recommendation-meta">
                    <span>{item.cleanable ? "未来版本可进入白名单清理流程" : item.blockedReason ?? "仅建议人工确认"}</span>
                    <span>{item.requiresAppClosed ? "需要先关闭相关应用" : "无需应用状态检查"}</span>
                    <span>置信度 {Math.round(item.confidence * 100)}%</span>
                    <span>{sourceLabels[item.source]}</span>
                  </div>
                </article>
              ))}
            </div>
          )}
        </section>
      </section>
    </main>
  );
}
