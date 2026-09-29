import type {
  Recommendation,
  RecommendationRisk,
  RecommendationSort,
  ScanReport,
  TreemapNodeDatum
} from "./types";

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
  if (
    lower.includes("\\windows defender") ||
    lower.includes("\\programdata\\microsoft\\windows defender")
  ) {
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
  const systemManagedCount = report.recommendations.filter(
    (item) => item.category === "system-managed"
  ).length;

  return [
    {
      id: "privacy",
      label: "隐私状态",
      detail: report.privacy.uploaded
        ? "报告标记为已上传，请暂停发布并检查实现。"
        : "报告只保存在本机，未标记上传。",
      tone: report.privacy.uploaded ? "bad" : "good"
    },
    {
      id: "recommendations",
      label: "建议数量",
      detail:
        report.recommendations.length > 0
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
      detail:
        report.scanErrors.length > 0
          ? `${report.scanErrors.length} 条路径读取受限，属于普通用户扫描的常见现象。`
          : "没有记录读取受限路径。",
      tone: report.scanErrors.length > 0 ? "warn" : "good"
    },
    {
      id: "reparse-points",
      label: "链接/虚拟目录",
      detail:
        report.skippedReparsePoints > 0
          ? `已跳过 ${report.skippedReparsePoints} 个重解析点，避免重复或虚拟占用。`
          : "没有记录跳过的重解析点；若机器上有云盘/手机镜像，请人工留意。",
      tone: report.skippedReparsePoints > 0 ? "good" : "warn"
    }
  ];
}

// ==== Phase 3 报告发现层：搜索 / 排序 / treemap / 历史对比 ====

export type { RecommendationSort };

export interface RecommendationFilter {
  category: Recommendation["category"] | "all";
  /** 路径子串过滤（大小写不敏感）。 */
  query: string;
  sort: RecommendationSort;
}

const riskRank: Record<RecommendationRisk, number> = {
  low: 0,
  medium: 1,
  high: 2,
  blocked: 3
};

/** 搜索 + 过滤 + 排序一体：sort= size|risk|confidence，平手按体量降序、路径升序。 */
export function filterAndSortRecommendations(
  recommendations: Recommendation[],
  filter: RecommendationFilter
): Recommendation[] {
  const query = filter.query.trim().toLowerCase();
  const filtered = recommendations.filter((item) => {
    if (filter.category !== "all" && item.category !== filter.category) {
      return false;
    }
    if (query && !item.path.toLowerCase().includes(query)) {
      return false;
    }
    return true;
  });
  return filtered.sort((left, right) => {
    if (filter.sort === "risk") {
      const riskDelta = riskRank[right.risk] - riskRank[left.risk];
      if (riskDelta !== 0) {
        return riskDelta;
      }
    }
    if (filter.sort === "confidence") {
      const confidenceDelta = right.confidence - left.confidence;
      if (confidenceDelta !== 0) {
        return confidenceDelta;
      }
    }
    const sizeDelta = right.sizeGb - left.sizeGb;
    if (sizeDelta !== 0) {
      return sizeDelta;
    }
    return left.path.localeCompare(right.path);
  });
}

const PATH_SEPARATOR = String.fromCharCode(92);

function lastPathSegment(path: string) {
  const parts = path.split(PATH_SEPARATOR).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

function isPathUnder(childPath: string, parentPath: string) {
  const normalize = (value: string) => value.toLowerCase().replace(/\\+$/, "");
  const child = normalize(childPath);
  const parent = normalize(parentPath);
  return child === parent || child.startsWith(`${parent}${PATH_SEPARATOR}`);
}

/** 挂载策略：找到路径前缀最深的已知节点；找不到则挂在根下。 */
function deepestAnchor(nodes: TreemapNodeDatum[], path: string): TreemapNodeDatum {
  let best = nodes[0];
  let bestLength = -1;
  for (const node of nodes) {
    if (isPathUnder(path, node.id) && node.id.length > bestLength) {
      best = node;
      bestLength = node.id.length;
    }
  }
  return best;
}

function capChildren(children: TreemapNodeDatum[], maxChildren: number): TreemapNodeDatum[] {
  if (children.length <= maxChildren) {
    return children;
  }
  const sorted = [...children].sort((left, right) => right.sizeGb - left.sizeGb);
  const kept = sorted.slice(0, maxChildren - 1);
  const rest = sorted.slice(maxChildren - 1);
  const restTotal = rest.reduce((sum, item) => sum + item.sizeGb, 0);
  kept.push({
    id: `${kept[0]?.id ?? "root"}::residual`,
    name: `其他 ${rest.length} 项`,
    sizeGb: restTotal,
    kind: "residual"
  });
  return kept;
}

/**
 * 构建 treemap 层级（只读聚合）：
 * 根 → 顶层目录行 → deep 深挖行 / 大文件叶子；子项和小于父体量时补「其他」残差保面积真实。
 * 每层最多保留 maxChildren 个矩形，溢出合并为残差（不静默丢弃，聚合进「其他 N 项」）。
 */
export function buildTreemapData(report: ScanReport, maxChildren = 12): TreemapNodeDatum {
  const root: TreemapNodeDatum = {
    id: report.drive || "drive",
    name: report.drive || "磁盘",
    sizeGb: 0,
    kind: "root",
    children: []
  };
  const known: TreemapNodeDatum[] = [root];

  const topRows = [...report.topRows].sort((left, right) => right.sizeGb - left.sizeGb);
  for (const row of topRows) {
    const node: TreemapNodeDatum = {
      id: row.path,
      name: lastPathSegment(row.path),
      sizeGb: row.sizeGb,
      kind: row.dirs === 0 && row.files > 0 ? "file" : "dir",
      children: row.dirs > 0 || row.files > 1 ? [] : undefined
    };
    if (node.children) {
      known.push(node);
    }
    root.children!.push(node);
  }

  for (const drill of report.drilldowns) {
    const parent = deepestAnchor(known, drill.root);
    if (parent === root && root.children!.some((child) => child.id === drill.root)) {
      continue;
    }
    let target = root.children!.find((child) => child.id === drill.root) as
      TreemapNodeDatum | undefined;
    if (!target) {
      target = {
        id: drill.root,
        name: lastPathSegment(drill.root),
        sizeGb: drill.rows.reduce((sum, row) => sum + row.sizeGb, 0),
        kind: "dir",
        children: []
      };
      known.push(target);
      (parent.children ??= []).push(target);
    }
    target.children ??= [];
    for (const row of [...drill.rows].sort((a, b) => b.sizeGb - a.sizeGb)) {
      if (target.children.some((child) => child.id === row.path)) {
        continue;
      }
      const node: TreemapNodeDatum = {
        id: row.path,
        name: lastPathSegment(row.path),
        sizeGb: row.sizeGb,
        kind: row.dirs === 0 ? "file" : "dir",
        children: row.dirs > 0 ? [] : undefined
      };
      if (node.children) {
        known.push(node);
      }
      target.children.push(node);
    }
    // 残差：深挖行覆盖不到的体量。
    const childTotal = target.children.reduce((sum, child) => sum + child.sizeGb, 0);
    const residual = Math.max(0, target.sizeGb - childTotal);
    if (residual > 0.005) {
      target.children.push({
        id: `${target.id}::residual`,
        name: "其他（未列出）",
        sizeGb: residual,
        kind: "residual"
      });
    }
    target.children = capChildren(target.children, maxChildren);
  }

  for (const file of report.largeFiles) {
    const parent = deepestAnchor(known, file.path);
    if (parent.kind === "file" || parent.id === file.path) {
      continue;
    }
    parent.children ??= [];
    if (parent.children.some((child) => child.id === file.path)) {
      continue;
    }
    parent.children.push({
      id: file.path,
      name: lastPathSegment(file.path),
      sizeGb: file.sizeGb,
      kind: "file"
    });
  }

  // 顶层残差与统一截断。
  root.children = capChildren(root.children!, maxChildren);
  const rootChildTotal = root.children.reduce((sum, child) => sum + child.sizeGb, 0);
  const rootResidual = Math.max(0, root.sizeGb - rootChildTotal);
  root.sizeGb = rootChildTotal + rootResidual;
  return root;
}

export interface ReportDeltaItem {
  path: string;
  previousGb: number;
  currentGb: number;
  deltaGb: number;
  change: "new" | "gone" | "grown" | "shrunken" | "unchanged";
}

function recommendationSizeMap(report: ScanReport) {
  const map = new Map<string, number>();
  for (const item of report.recommendations) {
    const previous = map.get(item.path) ?? 0;
    map.set(item.path, Math.max(previous, item.sizeGb));
  }
  return map;
}

/** 两次扫描对比（纯前端计算）：按建议路径对齐体量差异。 */
export function compareReports(
  previous: ScanReport,
  current: ScanReport,
  epsilon = 0.005
): ReportDeltaItem[] {
  const previousMap = recommendationSizeMap(previous);
  const currentMap = recommendationSizeMap(current);
  const paths = [...new Set([...previousMap.keys(), ...currentMap.keys()])];
  return paths
    .map((path) => {
      const previousGb = previousMap.get(path) ?? 0;
      const currentGb = currentMap.get(path) ?? 0;
      const deltaGb = currentGb - previousGb;
      let change: ReportDeltaItem["change"] = "unchanged";
      if (!previousMap.has(path)) {
        change = "new";
      } else if (!currentMap.has(path)) {
        change = "gone";
      } else if (deltaGb > epsilon) {
        change = "grown";
      } else if (deltaGb < -epsilon) {
        change = "shrunken";
      }
      return { path, previousGb, currentGb, deltaGb, change };
    })
    .sort((left, right) => {
      const absDelta = Math.abs(right.deltaGb) - Math.abs(left.deltaGb);
      if (absDelta !== 0) {
        return absDelta;
      }
      return left.path.localeCompare(right.path);
    });
}

/** treemap 叶子/目录行的体量标签（悬浮提示用）。 */
export function treemapNodeDetail(node: TreemapNodeDatum): string {
  if (node.kind === "residual") {
    return `聚合显示 ${formatSize(node.sizeGb)}`;
  }
  return `${formatSize(node.sizeGb)}（${node.id}）`;
}
