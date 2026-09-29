import { Activity, Clock3 } from "lucide-react";
import { formatDuration, scanStageDescription, scanStageInsight, scanTips } from "./scanStages";
import type { ScanActivityItem } from "./useScanSession";
import type { ScanMode } from "./types";

interface ScanCompanionProps {
  percent: number;
  mode: ScanMode;
  elapsedMs: number;
  currentStageMs: number;
  isProgressStalled: boolean;
  scanActivity: ScanActivityItem[];
}

/// 扫描陪伴面板：扫描进行中的阶段说明、计时与活动流（等待体验的唯一展示组件）。
export function ScanCompanion({
  percent,
  mode,
  elapsedMs,
  currentStageMs,
  isProgressStalled,
  scanActivity
}: ScanCompanionProps) {
  const tipIndex = Math.floor(Math.max(0, elapsedMs) / 15000) % scanTips.length;
  const stageInsight = scanStageInsight(percent, mode);

  return (
    <section className="scan-companion" aria-label="扫描陪伴">
      <div className="companion-mark" aria-hidden="true">
        <Activity size={22} />
      </div>
      <div className="companion-copy">
        <strong>{isProgressStalled ? "当前阶段仍在工作" : "扫描正在进行"}</strong>
        <span>{scanStageDescription(percent)}</span>
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
  );
}
