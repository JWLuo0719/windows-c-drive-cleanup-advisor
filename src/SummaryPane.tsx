import { ClipboardCopy, FileJson, FileText, FolderOpen, History } from "lucide-react";
import {
  categoryLabels,
  formatSize,
  summarizeScanErrors,
  buildReportHealthChecks
} from "./reportUtils";
import type { Recommendation, ScanReport } from "./types";

interface SummaryPaneProps {
  report: ScanReport | null;
  selectedCategory: Recommendation["category"] | "all";
  onSelectCategory: (category: Recommendation["category"] | "all") => void;
  groupedTotals: Map<Recommendation["category"], number>;
  totalReviewSize: number;
  scanErrorSummary: ReturnType<typeof summarizeScanErrors>;
  reportHealthChecks: ReturnType<typeof buildReportHealthChecks>;
  isWorking: boolean;
  copyNotice: string | null;
  onLoadRecent: () => void;
  onShowReport: (kind: "markdown" | "json" | "folder") => void;
  onCopyPaths: () => void;
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

/// 摘要侧栏：报告摘要、风险分组、扫描备注、结果自检、结果判读与本地报告入口。
export function SummaryPane({
  report,
  selectedCategory,
  onSelectCategory,
  groupedTotals,
  totalReviewSize,
  scanErrorSummary,
  reportHealthChecks,
  isWorking,
  copyNotice,
  onLoadRecent,
  onShowReport,
  onCopyPaths
}: SummaryPaneProps) {
  return (
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
          onClick={() => onSelectCategory("all")}
        >
          <span>全部建议</span>
          <strong>{report?.recommendations.length ?? 0}</strong>
        </button>
        {(Object.keys(categoryLabels) as Recommendation["category"][]).map((category) => (
          <button
            className={selectedCategory === category ? "filter-row selected" : "filter-row"}
            key={category}
            type="button"
            onClick={() => onSelectCategory(category)}
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
        <p>
          {report
            ? "生成的文件只保存在这台电脑上。"
            : "运行扫描后会生成 Markdown 和 JSON 报告，也可以载入最近一次本地报告。"}
        </p>
        <div className="path-pill">
          <FileText size={16} />
          <span>{report?.markdownReportPath ?? "Markdown 报告待生成"}</span>
        </div>
        <div className="path-pill">
          <FileJson size={16} />
          <span>{report?.jsonReportPath ?? "JSON 报告待生成"}</span>
        </div>
        <div className="report-actions">
          <button type="button" onClick={onLoadRecent} disabled={isWorking}>
            <History size={16} />
            载入最近报告
          </button>
          <button type="button" onClick={() => onShowReport("markdown")} disabled={!report}>
            <FileText size={16} />
            显示 Markdown
          </button>
          <button type="button" onClick={() => onShowReport("json")} disabled={!report}>
            <FileJson size={16} />
            显示 JSON
          </button>
          <button type="button" onClick={() => onShowReport("folder")} disabled={!report}>
            <FolderOpen size={16} />
            打开报告目录
          </button>
          <button type="button" onClick={onCopyPaths} disabled={!report}>
            <ClipboardCopy size={16} />
            复制报告路径
          </button>
        </div>
        {copyNotice ? <p className="copy-notice">{copyNotice}</p> : null}
      </section>
    </aside>
  );
}
