import { Activity, Search, Sparkles } from "lucide-react";
import { categoryLabels, formatSize, riskLabels, sourceLabels } from "./reportUtils";
import type {
  CleanupOutcome,
  CleanupPlan,
  Recommendation,
  RecommendationSort,
  ScanReport
} from "./types";

interface RecommendationListProps {
  report: ScanReport | null;
  filteredRecommendations: Recommendation[];
  selectedCategoryName: string;
  query: string;
  sort: RecommendationSort;
  /** Phase 5：清理勾选与 plan-first 面板（全部状态归 useScanSession）。 */
  selectedCandidateIds: string[];
  cleanupPlan: CleanupPlan | null;
  cleanupOutcome: CleanupOutcome | null;
  cleanupBusy: boolean;
  onQueryChange: (query: string) => void;
  onSortChange: (sort: RecommendationSort) => void;
  onToggleCandidate: (id: string) => void;
  onPlanCleanup: () => void;
  onExecuteCleanup: () => void;
  onDismissCleanup: () => void;
}

const sortOptions: Array<{ value: RecommendationSort; label: string }> = [
  { value: "size", label: "按大小" },
  { value: "risk", label: "按风险" },
  { value: "confidence", label: "按置信度" }
];

/// 空间建议列表：只读建议卡片、搜索/排序工具栏、plan-first 清理面板与空态提示。
export function RecommendationList({
  report,
  filteredRecommendations,
  selectedCategoryName,
  query,
  sort,
  selectedCandidateIds,
  cleanupPlan,
  cleanupOutcome,
  cleanupBusy,
  onQueryChange,
  onSortChange,
  onToggleCandidate,
  onPlanCleanup,
  onExecuteCleanup,
  onDismissCleanup
}: RecommendationListProps) {
  const planRejected = cleanupPlan?.rejected ?? [];
  const outcomeFailed = (cleanupOutcome?.results ?? []).filter(
    (item) => item.status !== "recycled"
  );

  return (
    <section className="recommendation-pane">
      <div className="pane-heading">
        <div>
          <h2>空间建议</h2>
          <p>扫描与报告始终只读；仅低风险缓存可选中后移入回收站（实验，确认前先出计划）。</p>
        </div>
        <div className="readonly-badge">
          <Sparkles size={16} />
          只读扫描
        </div>
      </div>

      {report ? (
        <div className="recommendation-toolbar" aria-label="建议搜索与排序">
          <label className="search-field">
            <Search size={16} />
            <input
              type="search"
              value={query}
              placeholder="按路径过滤，例如 AppData"
              onChange={(event) => onQueryChange(event.target.value)}
            />
          </label>
          <div className="sort-selector" role="group" aria-label="排序方式">
            {sortOptions.map((option) => (
              <button
                key={option.value}
                type="button"
                className={sort === option.value ? "selected" : ""}
                onClick={() => onSortChange(option.value)}
              >
                {option.label}
              </button>
            ))}
          </div>
          <span className="result-count">{filteredRecommendations.length} 条</span>
        </div>
      ) : null}

      {report && selectedCandidateIds.length > 0 && !cleanupPlan && !cleanupOutcome ? (
        <div className="cleanup-bar" role="group" aria-label="清理操作">
          <span>已选 {selectedCandidateIds.length} 项待评估</span>
          <button type="button" onClick={onPlanCleanup} disabled={cleanupBusy}>
            {cleanupBusy ? "正在生成计划…" : "计划清理"}
          </button>
        </div>
      ) : null}

      {cleanupPlan ? (
        <div className="cleanup-plan" aria-label="清理计划">
          <h3>清理计划（移入回收站）</h3>
          <p>
            将回收 {cleanupPlan.items.length} 项，合计 {formatSize(cleanupPlan.totalSizeGb)}
            ；回收站保守预算 {formatSize(cleanupPlan.recycleBudgetGb)}。
          </p>
          {cleanupPlan.items.length > 0 ? (
            <ul className="cleanup-rows">
              {cleanupPlan.items.map((item) => (
                <li key={item.id}>
                  <span className="cleanup-size">{formatSize(item.sizeGb)}</span>
                  <span className="cleanup-path">{item.path}</span>
                </li>
              ))}
            </ul>
          ) : (
            <p className="cleanup-hint">没有可通过校验的候选，无法执行。</p>
          )}
          {planRejected.length > 0 ? (
            <>
              <h4>已拒绝 {planRejected.length} 项</h4>
              <ul className="cleanup-rows rejected">
                {planRejected.map((item) => (
                  <li key={item.id}>
                    <span className="cleanup-path">{item.path || item.id}</span>
                    <span className="cleanup-reason">{item.reason}</span>
                  </li>
                ))}
              </ul>
            </>
          ) : null}
          <div className="cleanup-actions">
            {cleanupPlan.items.length > 0 ? (
              <button type="button" onClick={onExecuteCleanup} disabled={cleanupBusy}>
                {cleanupBusy ? "正在执行…" : `确认移入回收站（${cleanupPlan.items.length} 项）`}
              </button>
            ) : null}
            <button type="button" onClick={onDismissCleanup} disabled={cleanupBusy}>
              返回
            </button>
          </div>
          <p className="cleanup-hint">
            执行前会逐项复判系统托管清单与路径身份（拒绝 reparse 目标），全部结果写入审计日志。
          </p>
        </div>
      ) : null}

      {cleanupOutcome ? (
        <div className="cleanup-outcome" aria-label="清理结果">
          <h3>清理结果</h3>
          <p>
            回收 {cleanupOutcome.recycledCount} 项（{formatSize(cleanupOutcome.totalSizeGb)}
            ）；失败 {cleanupOutcome.failedCount} 项；拒绝 {cleanupOutcome.rejected.length} 项。
          </p>
          {outcomeFailed.length > 0 ? (
            <ul className="cleanup-rows rejected">
              {outcomeFailed.map((item) => (
                <li key={item.id}>
                  <span className="cleanup-path">{item.path}</span>
                  <span className="cleanup-reason">{item.message ?? "未知错误"}</span>
                </li>
              ))}
            </ul>
          ) : null}
          <p className="cleanup-hint">审计日志：{cleanupOutcome.actionLogPath}</p>
          <p className="cleanup-hint">报告数据已过期，建议重新扫描以刷新体量与候选。</p>
          <div className="cleanup-actions">
            <button type="button" onClick={onDismissCleanup}>
              关闭
            </button>
          </div>
        </div>
      ) : null}

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
          {filteredRecommendations.map((item) => {
            const canPick = item.cleanable && item.category === "low-risk-cache";
            return (
              <article className={`recommendation ${item.risk}`} key={item.id}>
                <div className="recommendation-topline">
                  <span className="size">{formatSize(item.sizeGb)}</span>
                  <span className="category">{categoryLabels[item.category]}</span>
                  <span className={`risk ${item.risk}`}>{riskLabels[item.risk]}</span>
                  {canPick ? (
                    <label className="candidate-pick">
                      <input
                        type="checkbox"
                        aria-label={`选择清理 ${item.path}`}
                        checked={selectedCandidateIds.includes(item.id)}
                        disabled={cleanupBusy}
                        onChange={() => onToggleCandidate(item.id)}
                      />
                      <span>移入回收站（实验）</span>
                    </label>
                  ) : null}
                </div>
                <h3>{item.path}</h3>
                <p>{item.reason}</p>
                <div className="manual-steps">
                  {item.manualSteps.map((step) => (
                    <span key={step}>{step}</span>
                  ))}
                </div>
                <div className="recommendation-meta">
                  <span>
                    {item.cleanable
                      ? item.category === "low-risk-cache"
                        ? "可选中后移入回收站（实验）"
                        : "未来版本可进入白名单清理流程"
                      : (item.blockedReason ?? "仅建议人工确认")}
                  </span>
                  <span>{item.requiresAppClosed ? "需要先关闭相关应用" : "无需应用状态检查"}</span>
                  <span>置信度 {Math.round(item.confidence * 100)}%</span>
                  <span>{sourceLabels[item.source]}</span>
                </div>
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}
