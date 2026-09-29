import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import type { ScanProgressEvent, ScanReport, ScanStatus } from "./types";

const mocks = vi.hoisted(() => ({
  cancelScan: vi.fn(),
  executeCleanup: vi.fn(),
  getActionLog: vi.fn(),
  getScanReport: vi.fn(),
  getScanStatus: vi.fn(),
  listReports: vi.fn(),
  loadLatestReport: vi.fn(),
  loadReportById: vi.fn(),
  listen: vi.fn(),
  planCleanup: vi.fn(),
  revealReport: vi.fn(),
  startScan: vi.fn(),
  writeText: vi.fn()
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: mocks.listen
}));

vi.mock("./tauri", () => ({
  cancelScan: mocks.cancelScan,
  executeCleanup: mocks.executeCleanup,
  getActionLog: mocks.getActionLog,
  getScanReport: mocks.getScanReport,
  getScanStatus: mocks.getScanStatus,
  listReports: mocks.listReports,
  loadLatestReport: mocks.loadLatestReport,
  loadReportById: mocks.loadReportById,
  planCleanup: mocks.planCleanup,
  revealReport: mocks.revealReport,
  startScan: mocks.startScan
}));

let progressHandler: ((event: { payload: ScanProgressEvent }) => void) | undefined;

const completedStatus: ScanStatus = {
  scanId: "scan-123",
  phase: "completed",
  percent: 100,
  message: "扫描完成。",
  markdownReportPath: "D:\\Project\\report\\scan.md",
  jsonReportPath: "D:\\Project\\report\\scan.json"
};

const mockReport: ScanReport = {
  schemaVersion: "1.0",
  scanId: "scan-123",
  createdAt: "2026-06-20T10:30:00.000Z",
  drive: "C",
  isElevated: false,
  skippedReparsePoints: 3,
  scanErrors: ["C:\\System Volume Information", "C:\\Windows\\System32\\locked.tmp"],
  privacy: { uploaded: false },
  markdownReportPath: "D:\\Project\\report\\scan.md",
  jsonReportPath: "D:\\Project\\report\\scan.json",
  topRows: [
    {
      path: "C:\\Users\\demo\\AppData",
      sizeGb: 12.5,
      files: 320,
      dirs: 40,
      skippedReparsePoints: 1
    },
    { path: "C:\\pagefile.sys", sizeGb: 8, files: 1, dirs: 0, skippedReparsePoints: 0 }
  ],
  drilldowns: [
    {
      root: "C:\\Users\\demo\\AppData",
      rows: [
        {
          path: "C:\\Users\\demo\\AppData\\Local\\Temp",
          sizeGb: 2.5,
          files: 80,
          dirs: 6,
          skippedReparsePoints: 0
        }
      ]
    }
  ],
  largeFiles: [{ path: "C:\\Users\\demo\\AppData\\Local\\Temp\\big.iso", sizeGb: 1.2 }],
  recommendations: [
    {
      id: "cache-temp",
      path: "C:\\Users\\demo\\AppData\\Local\\Temp",
      sizeGb: 2.5,
      category: "low-risk-cache",
      risk: "low",
      confidence: 0.9,
      reason: "常见临时缓存目录，建议先关闭相关应用后人工复核。",
      manualSteps: ["打开目录", "按修改时间排序", "只删除确认无用的临时文件"],
      cleanable: false,
      blockedReason: "v0.1 只读，不执行清理",
      cleanupMethod: "manual-review",
      requiresAppClosed: true,
      source: "scanner"
    },
    {
      id: "winsxs",
      path: "C:\\Windows\\WinSxS",
      sizeGb: 20,
      category: "system-managed",
      risk: "blocked",
      confidence: 1,
      reason: "系统组件存储目录，只能使用系统工具判断。",
      manualSteps: ["使用磁盘清理或 DISM", "不要手动删除目录内容"],
      cleanable: false,
      blockedReason: "系统托管项禁止自动处理",
      cleanupMethod: "system-tool-only",
      requiresAppClosed: false,
      source: "dism"
    },
    {
      id: "backup-vhdx",
      path: "C:\\Backup\\huge.vhdx",
      sizeGb: 30,
      category: "user-data",
      risk: "low",
      confidence: 0.5,
      reason: "单个超大文件，建议人工确认是否需要迁移。",
      manualSteps: ["确认文件用途", "迁移到其他磁盘"],
      cleanable: false,
      requiresAppClosed: false,
      source: "heuristic"
    },
    {
      // Phase 5 唯一可勾选候选：low-risk-cache 且 cleanable。
      id: "npm-cache",
      path: "C:\\Users\\demo\\AppData\\Local\\npm-cache",
      sizeGb: 1.5,
      category: "low-risk-cache",
      risk: "low",
      confidence: 0.82,
      reason: "包管理器缓存目录，安装时会重建。",
      manualSteps: ["关闭正在运行的安装任务"],
      cleanable: true,
      cleanupMethod: "recycle",
      requiresAppClosed: false,
      source: "scanner"
    }
  ]
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}
function renderApp() {
  mocks.listen.mockImplementation(
    (_eventName: string, handler: (event: { payload: ScanProgressEvent }) => void) => {
      progressHandler = handler;
      return Promise.resolve(vi.fn());
    }
  );
  return render(<App />);
}

describe("App", () => {
  beforeEach(() => {
    vi.useRealTimers();
    mocks.cancelScan.mockReset();
    mocks.executeCleanup.mockReset();
    mocks.getActionLog.mockReset();
    mocks.getScanReport.mockReset();
    mocks.getScanStatus.mockReset();
    mocks.listReports.mockReset();
    mocks.loadLatestReport.mockReset();
    mocks.loadReportById.mockReset();
    mocks.listen.mockReset();
    mocks.planCleanup.mockReset();
    mocks.revealReport.mockReset();
    mocks.startScan.mockReset();
    mocks.writeText.mockReset();
    // HistoryPanel 挂载即拉取历史列表，缺省为空避免未 mock 崩渲染。
    mocks.listReports.mockResolvedValue([]);
    mocks.getActionLog.mockResolvedValue([]);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: {
        writeText: mocks.writeText
      }
    });
    progressHandler = undefined;
  });

  it("shows the Chinese read-only safety ledger before scanning", () => {
    renderApp();

    expect(
      screen.getByRole("heading", { name: "C 盘空间诊断，先看清楚，再决定。" })
    ).toBeInTheDocument();
    expect(screen.getByLabelText("安全账本")).toHaveTextContent("零清理动作");
    expect(screen.getByLabelText("安全账本")).toHaveTextContent("零数据上传");
    expect(screen.getByLabelText("安全账本")).toHaveTextContent("系统项不自动处理");
    expect(screen.getByTitle("取消扫描")).toBeDisabled();
    expect(mocks.listen).toHaveBeenCalledWith("scan-progress", expect.any(Function));
  });

  it("ignores progress events before a scan is active", () => {
    renderApp();

    act(() => {
      progressHandler?.({
        payload: {
          scanId: "stale-scan",
          phase: "running",
          percent: 88,
          message: "stale progress"
        }
      });
    });

    expect(screen.queryByText("stale progress")).not.toBeInTheDocument();
    expect(screen.queryByText("88%")).not.toBeInTheDocument();
  });

  it("applies only matching progress events for the active scan", async () => {
    mocks.startScan.mockResolvedValue("scan-active");

    renderApp();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /C/ }));
      await Promise.resolve();
    });

    act(() => {
      progressHandler?.({
        payload: {
          scanId: "scan-active",
          phase: "running",
          percent: 42,
          message: "event progress"
        }
      });
    });

    expect(screen.getAllByText("event progress").length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText("42%")).toBeInTheDocument();

    act(() => {
      progressHandler?.({
        payload: {
          scanId: "stale-scan",
          phase: "running",
          percent: 90,
          message: "wrong scan progress"
        }
      });
    });

    expect(screen.queryByText("wrong scan progress")).not.toBeInTheDocument();
    expect(screen.getAllByText("event progress").length).toBeGreaterThanOrEqual(1);
  });

  it("keeps the scan companion active when progress stalls", async () => {
    vi.useFakeTimers();
    mocks.startScan.mockResolvedValue("scan-stall");
    mocks.getScanStatus.mockResolvedValue({
      scanId: "scan-stall",
      phase: "running",
      percent: 18,
      message: "正在扫描 C 盘顶层真实目录。"
    });

    renderApp();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /C/ }));
      await Promise.resolve();
    });

    expect(screen.getByRole("region", { name: "扫描陪伴" })).toHaveTextContent("扫描正在进行");
    expect(screen.getByRole("region", { name: "扫描陪伴" })).toHaveTextContent("已用时 0 秒");
    expect(screen.getByRole("region", { name: "扫描活动" })).toHaveTextContent("扫描已排队");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(18000);
    });

    expect(screen.getByRole("region", { name: "扫描陪伴" })).toHaveTextContent("当前阶段仍在工作");
    expect(screen.getByRole("region", { name: "扫描陪伴" })).toHaveTextContent("顶层真实目录");
    expect(screen.getByRole("region", { name: "扫描陪伴" })).toHaveTextContent("当前阶段");
    expect(screen.getByRole("region", { name: "扫描陪伴" })).toHaveTextContent("为什么慢");
    expect(screen.getByRole("region", { name: "扫描陪伴" })).toHaveTextContent("下一步");
    expect(screen.getByRole("region", { name: "扫描陪伴" })).toHaveTextContent("查找大文件");
    expect(screen.getByRole("region", { name: "扫描活动" })).toHaveTextContent(
      "正在扫描 C 盘顶层真实目录。"
    );

    await act(async () => {
      await vi.advanceTimersByTimeAsync(15000);
    });

    expect(screen.getByRole("region", { name: "扫描活动" })).toHaveTextContent("当前阶段仍在工作");
    expect(screen.getByRole("region", { name: "扫描活动" })).toHaveTextContent("顶层真实目录");
    vi.useRealTimers();
  });

  it("loads the report immediately when a completed progress event arrives", async () => {
    mocks.startScan.mockResolvedValue("scan-event-complete");
    mocks.getScanReport.mockResolvedValue({
      ...mockReport,
      scanId: "scan-event-complete"
    });

    renderApp();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /C/ }));
      await Promise.resolve();
    });

    await act(async () => {
      progressHandler?.({
        payload: {
          scanId: "scan-event-complete",
          phase: "completed",
          percent: 100,
          message: "event complete"
        }
      });
    });

    expect(mocks.getScanReport).toHaveBeenCalledWith("scan-event-complete");
    expect(await screen.findByText("C:\\Users\\demo\\AppData\\Local\\Temp")).toBeInTheDocument();
    expect(screen.getAllByText("event complete").length).toBeGreaterThanOrEqual(1);
  });
  it("starts a fixed read-only scan, loads a report, filters risks, and opens reports", async () => {
    vi.useFakeTimers();
    mocks.startScan.mockResolvedValue("scan-123");
    mocks.getScanStatus.mockResolvedValue(completedStatus);
    mocks.getScanReport.mockResolvedValue(mockReport);

    renderApp();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /开始扫描 C 盘/ }));
      await Promise.resolve();
    });

    expect(
      screen.getAllByText("扫描已排队。本次不会执行任何清理动作。").length
    ).toBeGreaterThanOrEqual(1);
    expect(mocks.startScan).toHaveBeenCalledWith({
      drive: "C",
      topCount: 30,
      largeFileMb: 200,
      scanMode: "quick"
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1200);
    });

    expect(screen.getByText("C:\\Users\\demo\\AppData\\Local\\Temp")).toBeInTheDocument();
    expect(
      screen.getByText("有 2 条路径因权限或系统保护无法读取，扫描已继续完成。")
    ).toBeInTheDocument();
    expect(screen.getByText("系统恢复/卷信息目录")).toBeInTheDocument();
    expect(screen.getByText("Windows 系统组件目录")).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "结果自检" })).toHaveTextContent("报告只保存在本机");
    expect(screen.getByRole("region", { name: "结果自检" })).toHaveTextContent("读取受限");
    expect(screen.getByRole("region", { name: "结果判读" })).toHaveTextContent("系统托管项只提示");
    expect(screen.getByLabelText("安全账本")).toHaveTextContent("3 个重解析点");

    fireEvent.click(screen.getByRole("button", { name: /系统托管项/ }));
    expect(screen.queryByText("C:\\Users\\demo\\AppData\\Local\\Temp")).not.toBeInTheDocument();
    expect(screen.getByText("C:\\Windows\\WinSxS")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /应用托管数据/ }));
    expect(screen.getByText("当前分组没有建议。")).toBeInTheDocument();
    expect(screen.queryByText("还没有加载报告。")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /显示 Markdown/ }));
    expect(mocks.revealReport).toHaveBeenCalledWith("scan-123", "markdown");

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /复制报告路径/ }));
      await Promise.resolve();
    });
    expect(mocks.writeText).toHaveBeenCalledWith(
      "Markdown: D:\\Project\\report\\scan.md\nJSON: D:\\Project\\report\\scan.json"
    );
    expect(screen.getByText("报告路径已复制。")).toBeInTheDocument();

    vi.useRealTimers();
  });

  it("uses the selected deep scan profile", async () => {
    mocks.startScan.mockResolvedValue("scan-deep");

    renderApp();
    fireEvent.click(screen.getByRole("button", { name: /完整扫描/ }));
    fireEvent.click(screen.getByRole("button", { name: /C/ }));

    expect(
      (await screen.findAllByText("扫描已排队。本次不会执行任何清理动作。")).length
    ).toBeGreaterThanOrEqual(1);
    expect(mocks.startScan).toHaveBeenCalledWith({
      drive: "C",
      topCount: 30,
      largeFileMb: 200,
      scanMode: "deep"
    });
  });

  it("loads the latest local report without starting a new scan", async () => {
    mocks.loadLatestReport.mockResolvedValue({
      ...mockReport,
      scanId: "scan-latest"
    });

    renderApp();

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /载入最近报告/ }));
      await Promise.resolve();
    });

    expect(mocks.startScan).not.toHaveBeenCalled();
    expect(mocks.loadLatestReport).toHaveBeenCalledTimes(1);
    expect(screen.getByText("已载入最近一次本地报告。")).toBeInTheDocument();
    expect(screen.getByText("C:\\Users\\demo\\AppData\\Local\\Temp")).toBeInTheDocument();
    expect(screen.getByText("100%")).toBeInTheDocument();

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /复制报告路径/ }));
      await Promise.resolve();
    });

    expect(mocks.writeText).toHaveBeenCalledWith(
      "Markdown: D:\\Project\\report\\scan.md\nJSON: D:\\Project\\report\\scan.json"
    );
  });

  it("cancels an active scan through the narrow IPC wrapper", async () => {
    mocks.startScan.mockResolvedValue("scan-cancel");
    mocks.cancelScan.mockResolvedValue({
      scanId: "scan-cancel",
      phase: "cancelled",
      percent: 5,
      message: "扫描已取消。"
    });

    renderApp();
    fireEvent.click(screen.getByRole("button", { name: /开始扫描 C 盘/ }));

    expect(
      (await screen.findAllByText("扫描已排队。本次不会执行任何清理动作。")).length
    ).toBeGreaterThanOrEqual(1);
    fireEvent.click(screen.getByTitle("取消扫描"));

    expect(await screen.findByText("扫描已取消。")).toBeInTheDocument();
    expect(mocks.cancelScan).toHaveBeenCalledWith("scan-cancel");
  });

  it("ignores stale polling responses after cancellation", async () => {
    vi.useFakeTimers();
    const pendingStatus = deferred<ScanStatus>();
    mocks.startScan.mockResolvedValue("scan-race");
    mocks.getScanStatus.mockReturnValue(pendingStatus.promise);
    mocks.cancelScan.mockResolvedValue({
      scanId: "scan-race",
      phase: "cancelled",
      percent: 100,
      message: "cancelled now"
    });

    renderApp();
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /C/ }));
      await Promise.resolve();
    });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1200);
    });
    expect(mocks.getScanStatus).toHaveBeenCalledWith("scan-race");

    await act(async () => {
      fireEvent.click(screen.getByTitle("取消扫描"));
      await Promise.resolve();
    });
    expect(screen.getAllByText("cancelled now").length).toBeGreaterThanOrEqual(1);

    await act(async () => {
      pendingStatus.resolve({
        scanId: "scan-race",
        phase: "running",
        percent: 77,
        message: "late running status"
      });
      await pendingStatus.promise;
    });

    expect(screen.queryByText("late running status")).not.toBeInTheDocument();
    expect(screen.getAllByText("cancelled now").length).toBeGreaterThanOrEqual(1);
    expect(mocks.getScanReport).not.toHaveBeenCalled();
    vi.useRealTimers();
  });
  it("surfaces start failures without leaving the scan in a working state", async () => {
    mocks.startScan.mockRejectedValue(new Error("PowerShell 不可用"));

    renderApp();
    fireEvent.click(screen.getByRole("button", { name: /开始扫描 C 盘/ }));

    expect(await screen.findByText("PowerShell 不可用")).toBeInTheDocument();
    expect(screen.getByText("准备进行只读扫描。")).toBeInTheDocument();
    expect(screen.getByTitle("取消扫描")).toBeDisabled();
  });

  it("surfaces cancel failures and keeps the running scan visible", async () => {
    mocks.startScan.mockResolvedValue("scan-cancel-error");
    mocks.cancelScan.mockRejectedValue(new Error("取消失败"));

    renderApp();
    fireEvent.click(screen.getByRole("button", { name: /开始扫描 C 盘/ }));

    expect(
      (await screen.findAllByText("扫描已排队。本次不会执行任何清理动作。")).length
    ).toBeGreaterThanOrEqual(1);
    fireEvent.click(screen.getByTitle("取消扫描"));

    expect(await screen.findByText("取消失败")).toBeInTheDocument();
    expect(screen.getByText("扫描 ID：scan-cancel-error")).toBeInTheDocument();
  });

  it("cleans up polling when the app unmounts", async () => {
    vi.useFakeTimers();
    const dispose = vi.fn();
    mocks.listen.mockResolvedValue(dispose);
    mocks.startScan.mockResolvedValue("scan-unmount");

    const { unmount } = render(<App />);
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /开始扫描 C 盘/ }));
      await Promise.resolve();
    });

    unmount();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(2400);
    });

    expect(mocks.getScanStatus).not.toHaveBeenCalled();
    expect(dispose).toHaveBeenCalled();
    vi.useRealTimers();
  });

  // ==== Phase 3 发现层 UI：treemap / 搜索排序 / 历史对比 ====

  it("renders the treemap discovery band when the report has tree rows", async () => {
    mocks.loadLatestReport.mockResolvedValue(mockReport);

    renderApp();
    fireEvent.click(screen.getByRole("button", { name: /载入最近报告/ }));

    expect(await screen.findByRole("img", { name: /只读体量图/ })).toBeInTheDocument();
    expect(screen.getByText(/下钻深度受报告数据限制/)).toBeInTheDocument();
  });

  it("filters recommendations by path query", async () => {
    mocks.loadLatestReport.mockResolvedValue(mockReport);

    renderApp();
    fireEvent.click(screen.getByRole("button", { name: /载入最近报告/ }));
    await screen.findByText("C:\\Users\\demo\\AppData\\Local\\Temp");

    fireEvent.change(screen.getByPlaceholderText("按路径过滤，例如 AppData"), {
      target: { value: "Temp" }
    });

    expect(screen.getByText("C:\\Users\\demo\\AppData\\Local\\Temp")).toBeInTheDocument();
    expect(screen.queryByText("C:\\Windows\\WinSxS")).not.toBeInTheDocument();
    expect(screen.queryByText("C:\\Backup\\huge.vhdx")).not.toBeInTheDocument();
    expect(within(screen.getByLabelText("建议搜索与排序")).getByText("1 条")).toBeInTheDocument();
  });

  it("reorders recommendations by risk and confidence", async () => {
    mocks.loadLatestReport.mockResolvedValue(mockReport);

    renderApp();
    fireEvent.click(screen.getByRole("button", { name: /载入最近报告/ }));
    await screen.findByText("C:\\Users\\demo\\AppData\\Local\\Temp");
    const listText = () => document.querySelector(".recommendation-list")?.textContent ?? "";

    // 默认按大小：30GB 备份 → 20GB WinSxS → 2.5GB Temp。
    expect(listText().indexOf("huge.vhdx")).toBeLessThan(listText().indexOf("WinSxS"));
    expect(listText().indexOf("WinSxS")).toBeLessThan(listText().indexOf("Local\\Temp"));

    fireEvent.click(screen.getByRole("button", { name: "按风险" }));
    // 风险降序：blocked WinSxS 第一，其余低风险按体量。
    expect(listText().indexOf("WinSxS")).toBeLessThan(listText().indexOf("huge.vhdx"));

    fireEvent.click(screen.getByRole("button", { name: "按置信度" }));
    // 置信度降序：WinSxS(100%) → Temp(90%) → 备份(50%)。
    expect(listText().indexOf("WinSxS")).toBeLessThan(listText().indexOf("Local\\Temp"));
    expect(listText().indexOf("Local\\Temp")).toBeLessThan(listText().indexOf("huge.vhdx"));
  });

  it("lists local history reports and loads a picked report", async () => {
    mocks.listReports.mockResolvedValue([
      {
        scanId: "scan-old",
        createdAt: "2026-09-20T10:00:00Z",
        drive: "C:",
        recommendationCount: 2,
        totalSizeGb: 3,
        scanErrorCount: 0,
        isLatest: false
      },
      {
        scanId: "scan-new",
        createdAt: "2026-09-25T10:00:00Z",
        drive: "C:",
        recommendationCount: 3,
        totalSizeGb: 5,
        scanErrorCount: 1,
        isLatest: true
      }
    ]);
    mocks.loadReportById.mockResolvedValue({ ...mockReport, scanId: "scan-new" });

    renderApp();
    expect(await screen.findByText("历史报告")).toBeInTheDocument();
    expect(screen.getByText("最新")).toBeInTheDocument();
    expect(screen.getByText(/2 条建议/)).toBeInTheDocument();

    fireEvent.click(screen.getAllByRole("button", { name: "载入" })[1]);

    expect(await screen.findByText("已载入选定历史报告。")).toBeInTheDocument();
    expect(mocks.loadReportById).toHaveBeenCalledWith("scan-new");
  });

  it("compares two picked history reports", async () => {
    const previousReport: ScanReport = {
      ...mockReport,
      scanId: "scan-old",
      createdAt: "2026-09-20T10:00:00Z",
      recommendations: [
        {
          ...mockReport.recommendations[0],
          sizeGb: 1
        }
      ]
    };
    mocks.listReports.mockResolvedValue([
      {
        scanId: "scan-old",
        createdAt: "2026-09-20T10:00:00Z",
        drive: "C:",
        recommendationCount: 1,
        totalSizeGb: 1,
        scanErrorCount: 0,
        isLatest: false
      },
      {
        scanId: "scan-new",
        createdAt: "2026-09-25T10:00:00Z",
        drive: "C:",
        recommendationCount: 3,
        totalSizeGb: 5,
        scanErrorCount: 0,
        isLatest: true
      }
    ]);
    mocks.loadReportById.mockImplementation((scanId: string) =>
      Promise.resolve(
        scanId === "scan-old"
          ? previousReport
          : { ...mockReport, scanId: "scan-new", createdAt: "2026-09-25T10:00:00Z" }
      )
    );

    renderApp();
    expect(await screen.findByText("历史报告")).toBeInTheDocument();
    for (const button of screen.getAllByRole("button", { name: "加入对比" })) {
      fireEvent.click(button);
    }
    fireEvent.click(screen.getByRole("button", { name: /生成对比/ }));

    expect(await screen.findByText(/共 \d+ 个路径变化/)).toBeInTheDocument();
    expect(screen.getAllByText("增长").length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText("新增").length).toBeGreaterThanOrEqual(1);
  });

  it("offers cleanup checkbox only for cleanable low-risk-cache candidates", async () => {
    mocks.loadLatestReport.mockResolvedValue(mockReport);

    renderApp();
    fireEvent.click(screen.getByRole("button", { name: /载入最近报告/ }));
    await screen.findByText("C:\\Users\\demo\\AppData\\Local\\Temp");

    // 可勾选：cleanable + low-risk-cache；不可勾选：不可清理缓存、系统项、用户数据。
    expect(
      screen.getByLabelText("选择清理 C:\\Users\\demo\\AppData\\Local\\npm-cache")
    ).toBeInTheDocument();
    expect(
      screen.queryByLabelText("选择清理 C:\\Users\\demo\\AppData\\Local\\Temp")
    ).not.toBeInTheDocument();
    expect(screen.queryByLabelText("选择清理 C:\\Windows\\WinSxS")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("选择清理 C:\\Backup\\huge.vhdx")).not.toBeInTheDocument();
    // 未勾选时不出现清理操作条。
    expect(screen.queryByRole("button", { name: "计划清理" })).not.toBeInTheDocument();
  });

  it("previews a cleanup plan with rejections, then executes and reports the outcome", async () => {
    mocks.loadLatestReport.mockResolvedValue(mockReport);
    mocks.planCleanup.mockResolvedValue({
      reportId: "scan-123",
      items: [
        {
          id: "npm-cache",
          path: "C:\\Users\\demo\\AppData\\Local\\npm-cache",
          sizeGb: 1.5,
          category: "low-risk-cache",
          cleanupMethod: "recycle"
        }
      ],
      rejected: [
        {
          id: "cache-temp",
          path: "C:\\Users\\demo\\AppData\\Local\\Temp",
          reason: "该候选不允许自动清理。"
        }
      ],
      totalSizeGb: 1.5,
      recycleBudgetGb: 12.5
    });
    mocks.executeCleanup.mockResolvedValue({
      reportId: "scan-123",
      results: [
        {
          id: "npm-cache",
          path: "C:\\Users\\demo\\AppData\\Local\\npm-cache",
          sizeGb: 1.5,
          status: "recycled",
          message: null
        }
      ],
      rejected: [],
      recycledCount: 1,
      failedCount: 0,
      totalSizeGb: 1.5,
      actionLogPath: "C:\\Users\\demo\\reports\\action-log.jsonl"
    });

    renderApp();
    fireEvent.click(screen.getByRole("button", { name: /载入最近报告/ }));
    await screen.findByText("C:\\Users\\demo\\AppData\\Local\\Temp");

    fireEvent.click(screen.getByLabelText("选择清理 C:\\Users\\demo\\AppData\\Local\\npm-cache"));
    fireEvent.click(screen.getByRole("button", { name: "计划清理" }));

    expect(await screen.findByText(/将回收 1 项/)).toBeInTheDocument();
    expect(mocks.planCleanup).toHaveBeenCalledWith("scan-123", ["npm-cache"]);
    expect(screen.getByText(/已拒绝 1 项/)).toBeInTheDocument();
    expect(screen.getByText("该候选不允许自动清理。")).toBeInTheDocument();
    expect(screen.getByText(/回收站保守预算/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /确认移入回收站/ }));

    // copyNotice 也会显示体量与过期提示，outcome 面板断言必须在面板内做。
    const outcomePanel = await screen.findByLabelText("清理结果");
    expect(within(outcomePanel).getByText(/失败 0 项/)).toBeInTheDocument();
    expect(mocks.executeCleanup).toHaveBeenCalledWith("scan-123", ["npm-cache"]);
    expect(
      within(outcomePanel).getByText(/审计日志：C:\\Users\\demo\\reports\\action-log\.jsonl/)
    ).toBeInTheDocument();
    expect(within(outcomePanel).getByText(/报告数据已过期/)).toBeInTheDocument();
    // 执行完成后勾选清空，操作条不再出现。
    expect(screen.queryByRole("button", { name: "计划清理" })).not.toBeInTheDocument();
  });

  it("lists cleanup audit log entries when the log section expands", async () => {
    mocks.getActionLog.mockResolvedValue([
      {
        ts: "2026-09-26T10:00:00Z",
        reportId: "scan-123",
        id: "npm-cache",
        path: "C:\\Users\\demo\\AppData\\Local\\npm-cache",
        sizeGb: 1.5,
        action: "recycle",
        status: "ok",
        message: null
      }
    ]);

    renderApp();
    fireEvent.click(screen.getByText("清理审计日志"));

    const logSection = await screen.findByLabelText("清理审计日志");
    expect(within(logSection).getByText("已回收")).toBeInTheDocument();
    expect(
      within(logSection).getByText("C:\\Users\\demo\\AppData\\Local\\npm-cache")
    ).toBeInTheDocument();
  });
});
