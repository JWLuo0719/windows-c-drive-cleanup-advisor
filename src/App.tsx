import { useMemo, useState } from "react";
import {
  Ban,
  CheckCircle2,
  FileText,
  FolderOpen,
  HardDrive,
  Loader2,
  PauseCircle,
  Play,
  ShieldCheck,
  XCircle
} from "lucide-react";
import {
  buildReportHealthChecks,
  buildTreemapData,
  categoryLabels,
  estimatePriorityReviewSize,
  filterAndSortRecommendations,
  groupRecommendationTotals,
  summarizeScanErrors
} from "./reportUtils";
import { scanProfiles } from "./scanStages";
import { useScanSession } from "./useScanSession";
import { ScanCompanion } from "./ScanCompanion";
import { SummaryPane } from "./SummaryPane";
import { RecommendationList } from "./RecommendationList";
import { TreemapView } from "./TreemapView";
import { HistoryPanel } from "./HistoryPanel";
import type { RecommendationSort, ScanStatus } from "./types";

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

/// 应用外壳：只保留命令栏、安全账本、进度条与错误带，
/// 会话状态归 useScanSession，展示归 ScanCompanion / SummaryPane / RecommendationList。
export function App() {
  const session = useScanSession();
  const {
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
    setScanMode,
    setSelectedCategory,
    selectedCandidateIds,
    cleanupPlan,
    cleanupOutcome,
    cleanupBusy,
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
  } = session;

  const [searchQuery, setSearchQuery] = useState("");
  const [sortMode, setSortMode] = useState<RecommendationSort>("size");

  const filteredRecommendations = useMemo(() => {
    return filterAndSortRecommendations(report?.recommendations ?? [], {
      category: selectedCategory,
      query: searchQuery,
      sort: sortMode
    });
  }, [report, selectedCategory, searchQuery, sortMode]);

  const treemapData = useMemo(() => {
    return report ? buildTreemapData(report) : null;
  }, [report]);

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
  const selectedCategoryName =
    selectedCategory === "all" ? "全部建议" : categoryLabels[selectedCategory];
  const selectedProfile = scanProfiles[scanMode];
  const elapsedMs = scanStartedAt ? clockTick - scanStartedAt : 0;
  const currentStageMs = lastProgressAt ? clockTick - lastProgressAt : 0;
  const isProgressStalled = isWorking && currentStageMs >= 15000;

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
            {(Object.keys(scanProfiles) as Array<keyof typeof scanProfiles>).map((mode) => (
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
          <button
            className="icon-action"
            type="button"
            onClick={stopScan}
            disabled={!isWorking}
            title="取消扫描"
          >
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
          <span>
            {status.scanId
              ? `扫描 ID：${status.scanId}`
              : `${selectedProfile.label}：${selectedProfile.detail}`}
          </span>
        </div>
        <div className="progress-block" aria-label="扫描进度">
          <div className="progress-track">
            <div style={{ width: `${safePercent}%` }} />
          </div>
          <span>{safePercent}%</span>
        </div>
      </section>

      {isWorking ? (
        <ScanCompanion
          percent={safePercent}
          mode={scanMode}
          elapsedMs={elapsedMs}
          currentStageMs={currentStageMs}
          isProgressStalled={isProgressStalled}
          scanActivity={scanActivity}
        />
      ) : null}

      {error ? (
        <section className="error-band">
          <XCircle size={20} />
          <span>{error}</span>
        </section>
      ) : null}

      {treemapData ? (
        <section className="discovery-band" aria-label="体量分布 treemap">
          <TreemapView data={treemapData} />
        </section>
      ) : null}

      <section className="workspace-grid">
        <SummaryPane
          report={report}
          selectedCategory={selectedCategory}
          onSelectCategory={setSelectedCategory}
          groupedTotals={groupedTotals}
          totalReviewSize={totalReviewSize}
          scanErrorSummary={scanErrorSummary}
          reportHealthChecks={reportHealthChecks}
          isWorking={isWorking}
          copyNotice={copyNotice}
          onLoadRecent={loadRecentReport}
          onShowReport={showReport}
          onCopyPaths={copyReportPaths}
        />

        <RecommendationList
          report={report}
          filteredRecommendations={filteredRecommendations}
          selectedCategoryName={selectedCategoryName}
          query={searchQuery}
          sort={sortMode}
          selectedCandidateIds={selectedCandidateIds}
          cleanupPlan={cleanupPlan}
          cleanupOutcome={cleanupOutcome}
          cleanupBusy={cleanupBusy}
          onQueryChange={setSearchQuery}
          onSortChange={setSortMode}
          onToggleCandidate={toggleCandidate}
          onPlanCleanup={planCleanup}
          onExecuteCleanup={executeCleanup}
          onDismissCleanup={dismissCleanup}
        />
      </section>

      <HistoryPanel currentScanId={report?.scanId ?? null} onLoad={loadReport} />
    </main>
  );
}
