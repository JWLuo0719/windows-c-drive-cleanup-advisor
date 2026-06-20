import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  AlertTriangle,
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
import { cancelScan, getScanReport, getScanStatus, startScan } from "./tauri";
import type { Recommendation, ScanProgressEvent, ScanReport, ScanStatus } from "./types";

const initialStatus: ScanStatus = {
  scanId: "",
  phase: "idle",
  percent: 0,
  message: "准备进行只读扫描。"
};

const categoryLabels: Record<Recommendation["category"], string> = {
  "low-risk-cache": "低风险缓存",
  "app-managed": "应用托管数据",
  "user-data": "需要用户判断",
  "uninstall-or-migrate": "卸载或迁移",
  "system-managed": "系统托管项"
};

const riskLabels: Record<Recommendation["risk"], string> = {
  low: "低风险",
  medium: "需确认",
  high: "高风险",
  blocked: "禁止自动处理"
};

const sourceLabels: Record<Recommendation["source"], string> = {
  scanner: "扫描器",
  dism: "DISM",
  registry: "注册表",
  heuristic: "启发式规则"
};

function formatSize(value: number) {
  if (!Number.isFinite(value)) {
    return "0.00 GB";
  }
  return `${value.toFixed(2)} GB`;
}

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

export function App() {
  const [status, setStatus] = useState<ScanStatus>(initialStatus);
  const [report, setReport] = useState<ScanReport | null>(null);
  const [selectedCategory, setSelectedCategory] = useState<Recommendation["category"] | "all">("all");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    listen<ScanProgressEvent>("scan-progress", (event) => {
      setStatus((current) => {
        if (current.scanId && current.scanId !== event.payload.scanId) {
          return current;
        }
        return {
          ...current,
          scanId: event.payload.scanId,
          phase: event.payload.phase,
          percent: event.payload.percent,
          message: event.payload.message
        };
      });
    }).then((dispose) => {
      unlisten = dispose;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  async function runScan() {
    setError(null);
    setReport(null);
    const scanId = await startScan({ drive: "C", topCount: 30, largeFileMb: 200 });
    setStatus({
      scanId,
      phase: "queued",
      percent: 2,
      message: "扫描已排队。本次不会执行任何清理动作。"
    });
    pollStatus(scanId);
  }

  async function pollStatus(scanId: string) {
    const timer = window.setInterval(async () => {
      try {
        const nextStatus = await getScanStatus(scanId);
        setStatus(nextStatus);
        if (nextStatus.phase === "completed") {
          window.clearInterval(timer);
          const nextReport = await getScanReport(scanId);
          setReport(nextReport);
        }
        if (["failed", "cancelled"].includes(nextStatus.phase)) {
          window.clearInterval(timer);
          if (nextStatus.error) {
            setError(nextStatus.error);
          }
        }
      } catch (err) {
        window.clearInterval(timer);
        setError(err instanceof Error ? err.message : String(err));
      }
    }, 1200);
  }

  async function stopScan() {
    if (!status.scanId) {
      return;
    }
    const nextStatus = await cancelScan(status.scanId);
    setStatus(nextStatus);
  }

  const filteredRecommendations = useMemo(() => {
    if (!report) {
      return [];
    }
    if (selectedCategory === "all") {
      return report.recommendations;
    }
    return report.recommendations.filter((item) => item.category === selectedCategory);
  }, [report, selectedCategory]);

  const groupedTotals = useMemo(() => {
    const totals = new Map<Recommendation["category"], number>();
    report?.recommendations.forEach((item) => {
      totals.set(item.category, (totals.get(item.category) ?? 0) + item.sizeGb);
    });
    return totals;
  }, [report]);

  const isWorking = status.phase === "queued" || status.phase === "running";

  return (
    <main className="app-shell">
      <section className="command-band">
        <div>
          <p className="eyebrow">Windows C 盘清理顾问</p>
          <h1>Windows C 盘空间诊断，先看清楚，再决定。</h1>
        </div>
        <div className="command-actions">
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
          <span>本次不执行清理</span>
        </div>
        <div>
          <Ban size={19} />
          <span>不上传任何数据</span>
        </div>
        <div>
          <FolderOpen size={19} />
          <span>跳过虚拟/重解析目录</span>
        </div>
        <div>
          <HardDrive size={19} />
          <span>系统托管项禁止自动处理</span>
        </div>
        <div>
          <FileText size={19} />
          <span>报告只保存在本机</span>
        </div>
      </section>

      <section className="scan-strip">
        <div className={`status-orb ${statusTone(status.phase)}`}>
          {status.phase === "completed" ? <CheckCircle2 size={25} /> : <HardDrive size={25} />}
        </div>
        <div className="status-copy">
          <strong>{status.message}</strong>
          <span>{status.scanId ? `扫描 ID ${status.scanId}` : "C 盘内容较多时，首次扫描可能需要几分钟。"}</span>
        </div>
        <div className="progress-block" aria-label="扫描进度">
          <div className="progress-track">
            <div style={{ width: `${Math.max(0, Math.min(100, status.percent))}%` }} />
          </div>
          <span>{status.percent}%</span>
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

          <div className="report-links">
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
          </div>
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

          {filteredRecommendations.length === 0 ? (
            <div className="empty-state">
              <AlertTriangle size={26} />
              <strong>还没有加载报告。</strong>
              <span>开始扫描后，这里会展示大文件夹、缓存候选和系统禁止项。</span>
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
