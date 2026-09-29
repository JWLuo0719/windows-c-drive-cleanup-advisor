import { describe, expect, it } from "vitest";
import {
  formatDuration,
  PROGRESS_PCT,
  scanStageDescription,
  scanStageInsight,
  scanTips
} from "./scanStages";

describe("scanStages", () => {
  it("formats durations as Chinese minute/second text", () => {
    expect(formatDuration(0)).toBe("0 秒");
    expect(formatDuration(5900)).toBe("5 秒");
    expect(formatDuration(65_000)).toBe("1 分 05 秒");
    expect(formatDuration(125_000)).toBe("2 分 05 秒");
    // 负值/非法输入夹到 0 秒，不产生负时间。
    expect(formatDuration(-500)).toBe("0 秒");
    expect(formatDuration(Number.NaN)).toBe("0 秒");
  });

  it("maps percent to stage descriptions across all boundaries", () => {
    expect(scanStageDescription(0)).toContain("准备扫描任务");
    expect(scanStageDescription(PROGRESS_PCT.DRIVE_INFO - 1)).toContain("准备扫描任务");
    expect(scanStageDescription(PROGRESS_PCT.DRIVE_INFO)).toContain("基础容量");
    expect(scanStageDescription(PROGRESS_PCT.TOP_ROOTS - 1)).toContain("基础容量");
    expect(scanStageDescription(PROGRESS_PCT.TOP_ROOTS)).toContain("顶层真实目录");
    expect(scanStageDescription(PROGRESS_PCT.LARGE_FILES - 1)).toContain("顶层真实目录");
    expect(scanStageDescription(PROGRESS_PCT.LARGE_FILES)).toContain("查找大文件");
    expect(scanStageDescription(PROGRESS_PCT.SYSTEM_INFO - 1)).toContain("查找大文件");
    expect(scanStageDescription(PROGRESS_PCT.SYSTEM_INFO)).toContain("系统状态");
    expect(scanStageDescription(PROGRESS_PCT.REPORT - 1)).toContain("系统状态");
    expect(scanStageDescription(PROGRESS_PCT.REPORT)).toContain("写入本地");
    expect(scanStageDescription(100)).toContain("写入本地");
  });

  it("maps percent to stage insights across all boundaries", () => {
    expect(scanStageInsight(0, "quick").focus).toBe("提交任务");
    expect(scanStageInsight(PROGRESS_PCT.DRIVE_INFO, "quick").focus).toBe("读取容量");
    expect(scanStageInsight(PROGRESS_PCT.TOP_ROOTS, "quick").focus).toBe("统计顶层目录");
    expect(scanStageInsight(PROGRESS_PCT.TOP_ROOTS, "quick").next).toBe("查找大文件");
    expect(scanStageInsight(PROGRESS_PCT.LARGE_FILES, "quick").focus).toBe("查找大文件");
    expect(scanStageInsight(PROGRESS_PCT.SYSTEM_INFO, "quick").focus).toBe("读取系统状态");
    expect(scanStageInsight(PROGRESS_PCT.REPORT, "quick").focus).toBe("生成报告");
    expect(scanStageInsight(100, "quick").next).toBe("展示结果");
  });

  it("distinguishes deep-mode drilldown insight from quick and early deep stages", () => {
    // deep 未到 DRILLDOWN_START：与 quick 相同的「统计顶层目录」，但 next 指向深挖。
    const earlyDeep = scanStageInsight(PROGRESS_PCT.TOP_ROOTS, "deep");
    expect(earlyDeep.focus).toBe("统计顶层目录");
    expect(earlyDeep.next).toBe("深挖重点目录");

    // deep 到达 DRILLDOWN_START：进入「深挖重点目录」。
    const drilldown = scanStageInsight(PROGRESS_PCT.DRILLDOWN_START, "deep");
    expect(drilldown.focus).toBe("深挖重点目录");
    expect(drilldown.next).toBe("查找大文件");

    // quick 即使百分比越过 DRILLDOWN_START 也不会进深挖分支。
    expect(scanStageInsight(PROGRESS_PCT.DRILLDOWN_START, "quick").focus).toBe("统计顶层目录");
  });

  it("keeps safety tips read-only and non-empty", () => {
    expect(scanTips.length).toBeGreaterThanOrEqual(3);
    expect(scanTips.some((tip) => tip.includes("不会删除"))).toBe(true);
  });
});
