import type { Recommendation, ScanReport } from "./types";

export const categoryLabels: Record<Recommendation["category"], string> = {
  "low-risk-cache": "低风险缓存",
  "app-managed": "应用托管数据",
  "user-data": "需要用户判断",
  "uninstall-or-migrate": "卸载或迁移",
  "system-managed": "系统托管项"
};

export const riskLabels: Record<Recommendation["risk"], string> = {
  low: "低风险",
  medium: "需确认",
  high: "高风险",
  blocked: "禁止自动处理"
};

export const sourceLabels: Record<Recommendation["source"], string> = {
  scanner: "扫描器",
  dism: "DISM",
  registry: "注册表",
  heuristic: "启发式规则"
};

export function formatSize(value: number) {
  if (!Number.isFinite(value)) {
    return "0.00 GB";
  }
  return `${value.toFixed(2)} GB`;
}

export function filterRecommendations(
  recommendations: Recommendation[],
  category: Recommendation["category"] | "all"
) {
  if (category === "all") {
    return recommendations;
  }
  return recommendations.filter((item) => item.category === category);
}

const reviewCategoryRank: Record<Recommendation["category"], number> = {
  "low-risk-cache": 0,
  "app-managed": 1,
  "user-data": 2,
  "uninstall-or-migrate": 3,
  "system-managed": 4
};

function isDriveRootSummary(path: string) {
  return /^[a-z]:\\[^\\]+$/i.test(path) || /^[a-z]:\\?$/i.test(path);
}

export function estimatePriorityReviewSize(recommendations: Recommendation[]) {
  return recommendations
    .filter((item) => item.category !== "system-managed")
    .filter((item) => !isDriveRootSummary(item.path))
    .reduce((sum, item) => sum + item.sizeGb, 0);
}

export function sortRecommendationsForReview(
  recommendations: Recommendation[],
  category: Recommendation["category"] | "all"
) {
  return [...recommendations].sort((left, right) => {
    if (category === "all") {
      const categoryDelta = reviewCategoryRank[left.category] - reviewCategoryRank[right.category];
      if (categoryDelta !== 0) {
        return categoryDelta;
      }
    }

    const leftIsRoot = isDriveRootSummary(left.path);
    const rightIsRoot = isDriveRootSummary(right.path);
    if (leftIsRoot !== rightIsRoot) {
      return leftIsRoot ? 1 : -1;
    }

    return right.sizeGb - left.sizeGb;
  });
}

export function groupRecommendationTotals(recommendations: Recommendation[]) {
  const totals = new Map<Recommendation["category"], number>();
  recommendations.forEach((item) => {
    totals.set(item.category, (totals.get(item.category) ?? 0) + item.sizeGb);
  });
  return totals;
}

export interface ScanErrorBucket {
  id: string;
  label: string;
  count: number;
  examples: string[];
}

function extractErrorPath(error: string) {
  const match = error.match(/:\s*([A-Z]:\\.*?)(?:\s+-|$)/i);
  return match?.[1] ?? error;
}

function scanErrorBucketFor(error: string): Pick<ScanErrorBucket, "id" | "label"> {
  const lower = error.toLowerCase();
  if (lower.includes("\\windows defender") || lower.includes("\\programdata\\microsoft\\windows defender")) {
    return { id: "defender", label: "Windows Defender 保护目录" };
  }
  if (lower.includes("\\$recycle.bin")) {
    return { id: "recycle-bin", label: "其他用户或系统回收站" };
  }
  if (lower.includes("\\windowsapps") || lower.includes("\\packages")) {
    return { id: "windows-apps", label: "Microsoft Store 应用目录" };
  }
  if (lower.includes("\\system volume information") || lower.includes("\\recovery")) {
    return { id: "system-protected", label: "系统恢复/卷信息目录" };
  }
  if (lower.includes("\\windows\\system32") || lower.includes("\\windows\\syswow64")) {
    return { id: "windows-system", label: "Windows 系统组件目录" };
  }
  if (lower.includes("\\programdata\\microsoft")) {
    return { id: "microsoft-programdata", label: "Microsoft ProgramData 目录" };
  }
  if (lower.includes("\\users\\")) {
    return { id: "user-protected", label: "其他用户或受保护用户目录" };
  }
  return { id: "other", label: "其他受限路径" };
}

export function summarizeScanErrors(errors: string[], previewLimit = 4) {
  const safeLimit = Math.max(0, previewLimit);
  const bucketMap = new Map<string, ScanErrorBucket>();

  errors.forEach((error) => {
    const bucketInfo = scanErrorBucketFor(error);
    const existing = bucketMap.get(bucketInfo.id) ?? {
      ...bucketInfo,
      count: 0,
      examples: []
    };
    existing.count += 1;
    if (existing.examples.length < 2) {
      existing.examples.push(extractErrorPath(error));
    }
    bucketMap.set(bucketInfo.id, existing);
  });

  const buckets = [...bucketMap.values()].sort((left, right) => {
    if (left.id === "other" && right.id !== "other") {
      return 1;
    }
    if (right.id === "other" && left.id !== "other") {
      return -1;
    }
    return right.count - left.count;
  });

  return {
    count: errors.length,
    preview: errors.slice(0, safeLimit),
    hasMore: errors.length > safeLimit,
    buckets
  };
}

export type ReportHealthTone = "good" | "warn" | "bad";

export interface ReportHealthCheck {
  id: string;
  label: string;
  detail: string;
  tone: ReportHealthTone;
}

export function buildReportHealthChecks(report: ScanReport): ReportHealthCheck[] {
  const hasUnsafeSystemManaged = report.recommendations.some((item) => {
    return item.category === "system-managed" && (item.cleanable || item.risk !== "blocked");
  });
  const systemManagedCount = report.recommendations.filter((item) => item.category === "system-managed").length;

  return [
    {
      id: "privacy",
      label: "隐私状态",
      detail: report.privacy.uploaded ? "报告标记为已上传，请暂停发布并检查实现。" : "报告只保存在本机，未标记上传。",
      tone: report.privacy.uploaded ? "bad" : "good"
    },
    {
      id: "recommendations",
      label: "建议数量",
      detail: report.recommendations.length > 0
        ? `已生成 ${report.recommendations.length} 条可人工复核的空间建议。`
        : "没有生成建议，可能磁盘压力较低，也可能需要检查扫描范围。",
      tone: report.recommendations.length > 0 ? "good" : "warn"
    },
    {
      id: "system-managed",
      label: "系统托管防线",
      detail: hasUnsafeSystemManaged
        ? "存在系统托管项被标记为可清理，请先修正分类。"
        : systemManagedCount > 0
          ? `${systemManagedCount} 个系统托管项均保持阻断建议。`
          : "本次没有识别到系统托管建议项。",
      tone: hasUnsafeSystemManaged ? "bad" : "good"
    },
    {
      id: "scan-errors",
      label: "读取受限记录",
      detail: report.scanErrors.length > 0
        ? `${report.scanErrors.length} 条路径读取受限，属于普通用户扫描的常见现象。`
        : "没有记录读取受限路径。",
      tone: report.scanErrors.length > 0 ? "warn" : "good"
    },
    {
      id: "reparse-points",
      label: "链接/虚拟目录",
      detail: report.skippedReparsePoints > 0
        ? `已跳过 ${report.skippedReparsePoints} 个重解析点，避免重复或虚拟占用。`
        : "没有记录跳过的重解析点；若机器上有云盘/手机镜像，请人工留意。",
      tone: report.skippedReparsePoints > 0 ? "good" : "warn"
    }
  ];
}
