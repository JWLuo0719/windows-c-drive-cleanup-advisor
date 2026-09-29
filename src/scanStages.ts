import type { ScanMode } from "./types";

// ==== 进度协议常量表 ====
// 阶段文案分段边界与进度协议百分比三方对齐（一致性由 scripts/Test-ScannerContract.ps1 断言，改动必须同步三处）：
// - scripts/Scan-CDriveCleanupAdvisor.ps1 的 $ProgressPercent（发射端）
// - src-tauri/src/scanner.rs 的 PROGRESS_PCT_*（解析端）
// UI 阶段判断只允许消费 percent，禁止解析 message 文案推断阶段（文案耦合已废除）。
export const PROGRESS_PCT = {
  DRIVE_INFO: 12,
  TOP_ROOTS: 18,
  DRILLDOWN_START: 35,
  LARGE_FILES: 65,
  SYSTEM_INFO: 74,
  DISM: 82,
  REPORT: 88,
  JSON: 94
} as const;

// quick/deep 的真实差异只有是否深挖常见目录（对应扫描器 -SkipCommonRoots 开关）；
// topCount / largeFileMb 两种模式共用同一默认值，收敛为单一常量，避免形同虚设的双份配置。
export const SCAN_DEFAULTS = {
  topCount: 30,
  largeFileMb: 200
} as const;

export const scanProfiles: Record<ScanMode, { label: string; detail: string }> = {
  quick: {
    label: "快速扫描",
    detail: "约数分钟，跳过重复深挖，适合日常检查"
  },
  deep: {
    label: "完整扫描",
    detail: "会额外深挖常见目录，结果更细但耗时更久"
  }
};

export const scanTips = [
  "进度不动通常表示当前目录仍在枚举，应用没有卡死。",
  "受保护目录读取失败是正常现象，扫描器会记录后继续。",
  "扫描只会读取和生成报告，不会删除、移动或上传文件。",
  "低风险缓存也需要先关闭相关应用，再人工复核。",
  "系统托管目录只给出提示，不会变成清理任务。"
];

export function formatDuration(ms: number) {
  // NaN/Infinity 夹到 0，避免把「NaN 秒」显示给用户。
  const safeMs = Number.isFinite(ms) ? ms : 0;
  const safeSeconds = Math.max(0, Math.floor(safeMs / 1000));
  const minutes = Math.floor(safeSeconds / 60);
  const seconds = safeSeconds % 60;
  if (minutes === 0) {
    return `${seconds} 秒`;
  }
  return `${minutes} 分 ${seconds.toString().padStart(2, "0")} 秒`;
}

export function scanStageDescription(percent: number) {
  if (percent < PROGRESS_PCT.DRIVE_INFO) {
    return "正在准备扫描任务和报告目录。";
  }
  if (percent < PROGRESS_PCT.TOP_ROOTS) {
    return "正在读取 C 盘基础容量信息。";
  }
  if (percent < PROGRESS_PCT.LARGE_FILES) {
    return "正在统计 C 盘顶层真实目录，这一步遇到大目录时会停留较久。";
  }
  if (percent < PROGRESS_PCT.SYSTEM_INFO) {
    return "正在查找大文件，文件数量多时进度可能暂时不变。";
  }
  if (percent < PROGRESS_PCT.REPORT) {
    return "正在读取系统状态并整理安全边界。";
  }
  return "正在写入本地 Markdown 和 JSON 报告。";
}

export function scanStageInsight(percent: number, mode: ScanMode) {
  // deep 模式深挖区间：DRILLDOWN 首点即 PROGRESS_PCT.DRILLDOWN_START，
  // 心跳百分比始终 ≥ 该边界，percent 是唯一判据（不再解析消息文案）。
  const isDeepDrilldown = mode === "deep" && percent >= PROGRESS_PCT.DRILLDOWN_START;

  if (percent < PROGRESS_PCT.DRIVE_INFO) {
    return {
      focus: "提交任务",
      reason: "正在建立只读扫描队列和本地报告位置。",
      next: "读取磁盘容量"
    };
  }
  if (percent < PROGRESS_PCT.TOP_ROOTS) {
    return {
      focus: "读取容量",
      reason: "需要确认 C 盘空间、权限和基础环境。",
      next: "统计顶层目录"
    };
  }
  if (percent < PROGRESS_PCT.LARGE_FILES) {
    if (isDeepDrilldown) {
      return {
        focus: "深挖重点目录",
        reason: "完整扫描会额外统计常见用户目录和 AppData。",
        next: "查找大文件"
      };
    }
    return {
      focus: "统计顶层目录",
      reason: "正在逐个计算真实目录体量，并跳过链接和受保护位置。",
      next: mode === "deep" ? "深挖重点目录" : "查找大文件"
    };
  }
  if (percent < PROGRESS_PCT.SYSTEM_INFO) {
    return {
      focus: "查找大文件",
      reason: "文件数量多时需要枚举候选路径，进度会小步推进。",
      next: "读取系统状态"
    };
  }
  if (percent < PROGRESS_PCT.REPORT) {
    return {
      focus: "读取系统状态",
      reason: "正在收集 pagefile、休眠和系统托管项的只读信息。",
      next: "生成报告"
    };
  }
  return {
    focus: "生成报告",
    reason: "正在把扫描结果写成本地 Markdown 和 JSON。",
    next: "展示结果"
  };
}
