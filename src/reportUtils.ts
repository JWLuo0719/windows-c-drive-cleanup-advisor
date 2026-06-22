import type { Recommendation } from "./types";

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

export function groupRecommendationTotals(recommendations: Recommendation[]) {
  const totals = new Map<Recommendation["category"], number>();
  recommendations.forEach((item) => {
    totals.set(item.category, (totals.get(item.category) ?? 0) + item.sizeGb);
  });
  return totals;
}

export function summarizeScanErrors(errors: string[], previewLimit = 4) {
  const safeLimit = Math.max(0, previewLimit);
  return {
    count: errors.length,
    preview: errors.slice(0, safeLimit),
    hasMore: errors.length > safeLimit
  };
}
