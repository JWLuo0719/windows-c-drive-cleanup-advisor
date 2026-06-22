import { describe, expect, it } from "vitest";
import {
  filterRecommendations,
  formatSize,
  groupRecommendationTotals,
  summarizeScanErrors
} from "./reportUtils";
import type { Recommendation } from "./types";

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

  it("summarizes scan errors without hiding the full count", () => {
    const summary = summarizeScanErrors(["A", "B", "C"], 2);

    expect(summary.count).toBe(3);
    expect(summary.preview).toEqual(["A", "B"]);
    expect(summary.hasMore).toBe(true);
  });
});
