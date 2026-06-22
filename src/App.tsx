import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  Activity,
  Ban,
  CheckCircle2,
  FileJson,
  FileText,
  FolderOpen,
  HardDrive,
  Loader2,
  PauseCircle,
  Play,
  ShieldCheck,
  Sparkles,
  XCircle
} from "lucide-react";
import {
  categoryLabels,
  filterRecommendations,
  formatSize,
  groupRecommendationTotals,
  riskLabels,
  sourceLabels,
  summarizeScanErrors
} from "./reportUtils";
import { cancelScan, getScanReport, getScanStatus, revealReport, startScan } from "./tauri";
import type { Recommendation, ScanProgressEvent, ScanReport, ScanStatus } from "./types";

const initialStatus: ScanStatus = {
  scanId: "",
  phase: "idle",
  percent: 0,
  message: "准备进行只读扫描。"
};

const terminalPhases = new Set<ScanStatus["phase"]>(["completed", "failed", "cancelled"]);

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

export function App() {
  const [status, setStatus] = useState<ScanStatus>(initialStatus);
  const [report, setReport] = useState<ScanReport | null>(null);
  const [selectedCategory, setSelectedCategory] = useState<Recommendation["category"] | "all">("all");
  const [error, setError] = useState<string | null>(null);
  const activeScanIdRef = useRef<string | null>(null);
  const pollTimerRef = useRef<number | null>(null);

  function clearPollTimer() {
    if (pollTimerRef.current !== null) {
      window.clearInterval(pollTimerRef.current);
      pollTimerRef.current = null;
    }
  }

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<ScanProgressEvent>("scan-progress", (event) => {
      const progress = event.payload;
      if (!activeScanIdRef.current || activeScanIdRef.current !== progress.scanId) {
        return;
      }
      setStatus((current) => {
        if (current.scanId && current.scanId !== progress.scanId) {
          return current;
        }
        if (terminalPhases.has(current.phase) && !terminalPhases.has(progress.phase)) {
          return current;
        }
        return {
          ...current,
          scanId: progress.scanId,
          phase: progress.phase,
          percent: progress.percent,
          message: progress.message
        };
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
      setReport(nextReport);
    } catch (err) {
      if (activeScanIdRef.current !== scanId) {
        return;
      }
      activeScanIdRef.current = null;
      setError(getErrorMessage(err));
    }
  }
  async function runScan() {
    clearPollTimer();
    activeScanIdRef.current = null;
    setError(null);
    setReport(null);
    setSelectedCategory("all");
    setStatus({
      scanId: "",
      phase: "queued",
      percent: 1,
      message: "正在提交只读扫描任务。"
    });

    try {
      const scanId = await startScan({ drive: "C", topCount: 30, largeFileMb: 200 });
      activeScanIdRef.current = scanId;
      setStatus({
        scanId,
        phase: "queued",
        percent: 2,
        message: "扫描已排队。本次不会执行任何清理动作。"
      });
      pollStatus(scanId);
    } catch (err) {
      activeScanIdRef.current = null;
      setStatus(initialStatus);
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
        setStatus(nextStatus);
        if (nextStatus.phase === "completed") {
          await loadCompletedReport(scanId);
        }
        if (["failed", "cancelled"].includes(nextStatus.phase)) {
          clearPollTimer();
          activeScanIdRef.current = null;
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
      setStatus(nextStatus);
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

  const filteredRecommendations = useMemo(() => {
    return filterRecommendations(report?.recommendations ?? [], selectedCategory);
  }, [report, selectedCategory]);

  const groupedTotals = useMemo(() => {
    return groupRecommendationTotals(report?.recommendations ?? []);
  }, [report]);

  const totalReviewSize = useMemo(() => {
    return (report?.recommendations ?? []).reduce((sum, item) => sum + item.sizeGb, 0);
  }, [report]);

  const scanErrorSummary = useMemo(() => {
    return summarizeScanErrors(report?.scanErrors ?? []);
  }, [report]);

  const isWorking = status.phase === "queued" || status.phase === "running";
  const safePercent = Math.max(0, Math.min(100, status.percent));
  const selectedCategoryName = selectedCategory === "all" ? "全部建议" : categoryLabels[selectedCategory];

  return (
    <main className="app-shell">
      <section className="command-band">
        <div className="command-copy">
          <p className="eyebrow">Windows C 盘清理顾问</p>
          <h1>C 盘空间诊断，先看清楚，再决定。</h1>
          <p className="intro">
            这是一套只读检查台：扫描真实本地目录、跳过重解析点、生成本机报告，并把清理建议交给你人工判断。
          </p>
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
          <span>v0.1 只读扫描</span>
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
          <span>{status.scanId ? `扫描 ID：${status.scanId}` : "C 盘内容较多时，首次扫描可能需要几分钟。"}</span>
        </div>
        <div className="progress-block" aria-label="扫描进度">
          <div className="progress-track">
            <div style={{ width: `${safePercent}%` }} />
          </div>
          <span>{safePercent}%</span>
        </div>
      </section>

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
              <span>待复核空间</span>
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
                <ul>
                  {scanErrorSummary.preview.map((item) => (
                    <li key={item}>{item}</li>
                  ))}
                </ul>
                {scanErrorSummary.hasMore ? <p>更多受限路径已写入本地报告。</p> : null}
              </>
            )}
          </section>

          <section className="report-links" aria-label="本地报告">
            <h2>本地报告</h2>
            <p>{report ? "生成的文件只保存在这台电脑上。" : "运行扫描后会生成 Markdown 和 JSON 报告。"}</p>
            <div className="path-pill">
              <FileText size={16} />
              <span>{report?.markdownReportPath ?? "Markdown 报告待生成"}</span>
            </div>
            <div className="path-pill">
              <FileJson size={16} />
              <span>{report?.jsonReportPath ?? "JSON 报告待生成"}</span>
            </div>
            <div className="report-actions">
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
            </div>
          </section>
        </aside>

        <section className="recommendation-pane">
          <div className="pane-heading">
            <div>
              <h2>空间建议</h2>
              <p>v0.1 只提供判断依据和手动步骤，清理能力会在后续版本单独上线。</p>
            </div>
            <div className="readonly-badge">
              <Sparkles size={16} />
              v0.1 只读
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
