import { describe, expect, it } from "vitest";
import {
  buildReportHealthChecks,
  estimatePriorityReviewSize,
  filterRecommendations,
  formatSize,
  groupRecommendationTotals,
  sortRecommendationsForReview,
  summarizeScanErrors
} from "./reportUtils";
import type { Recommendation, ScanReport } from "./types";

function recommendation(
  id: string,
  category: Recommendation["category"],
  sizeGb: number
): Recommendation {
  return {
    id,
    path: `C:\\demo\\${id}`,
    sizeGb,
    category,
    risk: category === "system-managed" ? "blocked" : "medium",
    confidence: 0.8,
    reason: "测试建议",
    manualSteps: ["人工确认"],
    cleanable: false,
    cleanupMethod: "manual-review",
    requiresAppClosed: false,
    source: "scanner"
  };
}

function report(overrides: Partial<ScanReport> = {}): ScanReport {
  return {
    schemaVersion: "1.0",
    scanId: "scan-test",
    createdAt: "2026-06-24T00:00:00.000Z",
    drive: "C",
    isElevated: false,
    skippedReparsePoints: 2,
    scanErrors: ["C:\\WindowsApps"],
    privacy: { uploaded: false },
    markdownReportPath: "D:\\report\\scan.md",
    jsonReportPath: "D:\\report\\scan.json",
    recommendations: [
      recommendation("cache", "low-risk-cache", 1.5),
      {
        ...recommendation("winsxs", "system-managed", 20),
        risk: "blocked",
        cleanable: false,
        blockedReason: "系统托管项禁止自动处理"
      }
    ],
    ...overrides
  };
}

describe("report utilities", () => {
  it("formats invalid and valid sizes consistently", () => {
    expect(formatSize(Number.NaN)).toBe("0.00 GB");
    expect(formatSize(12.345)).toBe("12.35 GB");
  });

  it("filters recommendations by category", () => {
    const items = [
      recommendation("cache", "low-risk-cache", 1.5),
      recommendation("system", "system-managed", 20)
    ];

    expect(filterRecommendations(items, "all")).toHaveLength(2);
    expect(filterRecommendations(items, "system-managed")).toEqual([items[1]]);
  });

  it("groups recommendation totals by risk category", () => {
    const totals = groupRecommendationTotals([
      recommendation("cache-a", "low-risk-cache", 1.25),
      recommendation("cache-b", "low-risk-cache", 2.75),
      recommendation("user", "user-data", 3)
    ]);

    expect(totals.get("low-risk-cache")).toBe(4);
    expect(totals.get("user-data")).toBe(3);
    expect(totals.get("system-managed")).toBeUndefined();
  });

  it("sorts all recommendations toward actionable review order", () => {
    const items = [
      recommendation("users-root", "user-data", 70),
      recommendation("windows", "system-managed", 35),
      recommendation("cache", "low-risk-cache", 4),
      recommendation("app", "app-managed", 8),
      recommendation("toolkit", "uninstall-or-migrate", 20)
    ];
    items[0].path = "C:\\Users";
    items[1].path = "C:\\Windows";

    expect(sortRecommendationsForReview(items, "all").map((item) => item.id)).toEqual([
      "cache",
      "app",
      "users-root",
      "toolkit",
      "windows"
    ]);
  });

  it("keeps root summaries behind specific paths inside a selected group", () => {
    const items = [
      recommendation("users-root", "user-data", 70),
      recommendation("download", "user-data", 1.5)
    ];
    items[0].path = "C:\\Users";
    items[1].path = "C:\\Users\\demo\\Downloads\\archive.rar";

    expect(sortRecommendationsForReview(items, "user-data").map((item) => item.id)).toEqual([
      "download",
      "users-root"
    ]);
  });

  it("estimates priority review size without root summaries or system-managed items", () => {
    const items = [
      recommendation("users-root", "user-data", 70),
      recommendation("download", "user-data", 1.5),
      recommendation("cache", "low-risk-cache", 4),
      recommendation("windows", "system-managed", 35)
    ];
    items[0].path = "C:\\Users";
    items[3].path = "C:\\Windows";

    expect(estimatePriorityReviewSize(items)).toBe(5.5);
  });

  it("summarizes scan errors without hiding the full count", () => {
    const summary = summarizeScanErrors(["A", "B", "C"], 2);

    expect(summary.count).toBe(3);
    expect(summary.preview).toEqual(["A", "B"]);
    expect(summary.hasMore).toBe(true);
  });

  it("groups expected protected scan errors into readable buckets", () => {
    const summary = summarizeScanErrors([
      "TreeSizeEnumerate: C:\\ProgramData\\Microsoft\\Windows Defender\\Scans - access denied",
      "TreeSizeEnumerate: C:\\Windows\\System32\\LogFiles\\WMI - access denied",
      "TreeSizeEnumerate: C:\\$Recycle.Bin\\S-1-5-18 - access denied"
    ]);

    expect(summary.buckets.map((bucket) => bucket.label)).toEqual([
      "Windows Defender 保护目录",
      "Windows 系统组件目录",
      "其他用户或系统回收站"
    ]);
    expect(summary.buckets[0].examples[0]).toBe("C:\\ProgramData\\Microsoft\\Windows Defender\\Scans");
  });

  it("builds report health checks for a normal advisory report", () => {
    const checks = buildReportHealthChecks(report());

    expect(checks.find((item) => item.id === "privacy")?.tone).toBe("good");
    expect(checks.find((item) => item.id === "system-managed")?.tone).toBe("good");
    expect(checks.find((item) => item.id === "scan-errors")?.tone).toBe("warn");
    expect(checks.find((item) => item.id === "reparse-points")?.detail).toContain("2 个重解析点");
  });

  it("flags suspicious report health states", () => {
    const checks = buildReportHealthChecks(report({
      privacy: { uploaded: true },
      skippedReparsePoints: 0,
      scanErrors: [],
      recommendations: [{
        ...recommendation("windows", "system-managed", 40),
        risk: "medium",
        cleanable: true
      }]
    }));

    expect(checks.find((item) => item.id === "privacy")?.tone).toBe("bad");
    expect(checks.find((item) => item.id === "system-managed")?.tone).toBe("bad");
    expect(checks.find((item) => item.id === "reparse-points")?.tone).toBe("warn");
  });

  it("warns when a report contains no recommendations", () => {
    const checks = buildReportHealthChecks(report({ recommendations: [] }));

    expect(checks.find((item) => item.id === "recommendations")?.tone).toBe("warn");
  });
});
