import { useCallback, useEffect, useMemo, useState } from "react";
import { History, RefreshCw, Scale } from "lucide-react";
import { getActionLog, listReports, loadReportById } from "./tauri";
import { compareReports, formatSize } from "./reportUtils";
import type { ReportDeltaItem } from "./reportUtils";
import type { ActionLogEntry, ReportSummary, ScanReport } from "./types";

interface HistoryPanelProps {
  currentScanId: string | null;
  onLoad: (scanId: string) => Promise<void>;
}

const changeLabels: Record<ReportDeltaItem["change"], string> = {
  new: "新增",
  gone: "已消失",
  grown: "增长",
  shrunken: "缩小",
  unchanged: "持平"
};

const DELTA_PREVIEW_LIMIT = 30;

function formatCreatedAt(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return new Intl.DateTimeFormat("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit"
  }).format(date);
}

/// 历史报告面板：本地报告列表、按 ID 载入，以及任意两次扫描的只读体量对比。
export function HistoryPanel({ currentScanId, onLoad }: HistoryPanelProps) {
  const [entries, setEntries] = useState<ReportSummary[]>([]);
  const [picked, setPicked] = useState<string[]>([]);
  const [comparison, setComparison] = useState<{ items: ReportDeltaItem[]; label: string } | null>(
    null
  );
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [actionLog, setActionLog] = useState<ActionLogEntry[]>([]);

  const refresh = useCallback(async () => {
    setNotice(null);
    try {
      setEntries(await listReports());
    } catch (err) {
      setNotice(err instanceof Error ? err.message : String(err));
    }
  }, []);

  // 挂载拉取走纯 Promise 回调，避免 effect 内同步 setState（react-hooks/set-state-in-effect）。
  useEffect(() => {
    let disposed = false;
    listReports()
      .then((items) => {
        if (!disposed) {
          setEntries(items);
        }
      })
      .catch((err) => {
        if (!disposed) {
          setNotice(err instanceof Error ? err.message : String(err));
        }
      });
    return () => {
      disposed = true;
    };
  }, []);

  const togglePick = (scanId: string) => {
    setComparison(null);
    setPicked((current) => {
      if (current.includes(scanId)) {
        return current.filter((item) => item !== scanId);
      }
      // 最多两条：满员时挤掉最早选中的那条。
      return [...current, scanId].slice(-2);
    });
  };

  const buildComparison = async () => {
    if (picked.length !== 2) {
      return;
    }
    setBusy(true);
    setNotice(null);
    try {
      const [first, second] = await Promise.all([
        loadReportById(picked[0]),
        loadReportById(picked[1])
      ]);
      const [previous, current]: [ScanReport, ScanReport] =
        first.createdAt <= second.createdAt ? [first, second] : [second, first];
      setComparison({
        items: compareReports(previous, current),
        label: `${formatCreatedAt(previous.createdAt)} → ${formatCreatedAt(current.createdAt)}`
      });
    } catch (err) {
      setNotice(err instanceof Error ? err.message : String(err));
    } finally {
      setBusy(false);
    }
  };

  const deltaPreview = useMemo(
    () => (comparison ? comparison.items.slice(0, DELTA_PREVIEW_LIMIT) : []),
    [comparison]
  );

  /// 清理审计日志：展开时拉取（每次展开刷新，保证执行清理后可见新记录）。
  const refreshActionLog = async () => {
    try {
      setActionLog(await getActionLog());
    } catch (err) {
      setNotice(err instanceof Error ? err.message : String(err));
    }
  };

  return (
    <section className="history-panel" aria-label="历史报告与对比">
      <div className="pane-heading">
        <div>
          <h2>
            <History size={18} aria-hidden /> 历史报告
          </h2>
          <p>只读列表来自本机 reports 目录，最多保留 20 次；对比仅计算已写入报告的建议体量。</p>
        </div>
        <button
          type="button"
          className="history-refresh"
          onClick={() => void refresh()}
          title="刷新历史列表"
        >
          <RefreshCw size={15} />
          刷新
        </button>
      </div>

      {entries.length === 0 ? (
        <div className="empty-state">
          <History size={26} />
          <strong>还没有历史报告。</strong>
          <span>完成一次扫描后，报告会出现在这里，供回看与多次扫描对比。</span>
        </div>
      ) : (
        <div className="history-list">
          {entries.map((entry) => {
            const isPicked = picked.includes(entry.scanId);
            const isCurrent = entry.scanId === currentScanId;
            return (
              <article className={`history-row${isPicked ? " picked" : ""}`} key={entry.scanId}>
                <div className="history-meta">
                  <strong>{formatCreatedAt(entry.createdAt)}</strong>
                  <span>
                    {entry.drive} · {entry.recommendationCount} 条建议 · 共{" "}
                    {formatSize(entry.totalSizeGb)}
                    {entry.scanErrorCount > 0 ? ` · ${entry.scanErrorCount} 条受限` : ""}
                  </span>
                </div>
                <div className="history-actions">
                  {entry.isLatest ? <span className="history-badge">最新</span> : null}
                  {isCurrent ? <span className="history-badge current">已载入</span> : null}
                  <button type="button" onClick={() => void togglePick(entry.scanId)}>
                    {isPicked ? "取消对比" : "加入对比"}
                  </button>
                  <button
                    type="button"
                    className="primary-mini"
                    onClick={() => void onLoad(entry.scanId)}
                  >
                    载入
                  </button>
                </div>
              </article>
            );
          })}
        </div>
      )}

      <div className="history-compare-bar">
        <button
          type="button"
          className="primary-mini"
          disabled={busy || picked.length !== 2}
          onClick={() => void buildComparison()}
        >
          <Scale size={15} />
          {busy ? "正在对比…" : `生成对比（已选 ${picked.length}/2）`}
        </button>
        {comparison ? (
          <span className="result-count">
            {comparison.label}：共 {comparison.items.length} 个路径变化
          </span>
        ) : (
          <span className="result-count">勾选两次扫描的报告即可对比体量变化</span>
        )}
      </div>

      <details
        className="action-log"
        aria-label="清理审计日志"
        onToggle={(event) => {
          if ((event.currentTarget as HTMLDetailsElement).open) {
            void refreshActionLog();
          }
        }}
      >
        <summary>清理审计日志</summary>
        {actionLog.length === 0 ? (
          <p className="treemap-hint">
            暂无清理记录。清理执行后，每次移入回收站的动作都会记在这里。
          </p>
        ) : (
          <ul className="action-log-list">
            {actionLog.map((entry, index) => (
              <li key={`${entry.ts}-${entry.id}-${index}`} className={`log-${entry.status}`}>
                <span className="action-log-time">{formatCreatedAt(entry.ts)}</span>
                <span className="action-log-status">
                  {entry.status === "ok" ? "已回收" : entry.status}
                </span>
                <span className="action-log-path" title={entry.path}>
                  {entry.path}
                </span>
                <span className="action-log-size">{formatSize(entry.sizeGb)}</span>
                {entry.message ? <span className="action-log-message">{entry.message}</span> : null}
              </li>
            ))}
          </ul>
        )}
      </details>

      {notice ? (
        <p className="history-notice" role="status">
          {notice}
        </p>
      ) : null}

      {comparison ? (
        <div className="history-delta">
          <table>
            <thead>
              <tr>
                <th>变化</th>
                <th>路径</th>
                <th>上次</th>
                <th>本次</th>
                <th>差值</th>
              </tr>
            </thead>
            <tbody>
              {deltaPreview.map((item) => (
                <tr key={item.path} className={`delta-${item.change}`}>
                  <td>{changeLabels[item.change]}</td>
                  <td title={item.path}>{item.path}</td>
                  <td>{item.previousGb > 0 ? formatSize(item.previousGb) : "—"}</td>
                  <td>{item.currentGb > 0 ? formatSize(item.currentGb) : "—"}</td>
                  <td>
                    {item.deltaGb > 0 ? "+" : ""}
                    {item.deltaGb.toFixed(2)} GB
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {comparison.items.length > DELTA_PREVIEW_LIMIT ? (
            <p className="treemap-hint">
              仅展示体量变化最大的 {DELTA_PREVIEW_LIMIT} 项（共 {comparison.items.length}{" "}
              项），完整数据见两份 JSON 报告。
            </p>
          ) : null}
        </div>
      ) : null}
    </section>
  );
}
