import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import type { ScanProgressEvent, ScanReport, ScanStatus } from "./types";

const mocks = vi.hoisted(() => ({
  cancelScan: vi.fn(),
  getScanReport: vi.fn(),
  getScanStatus: vi.fn(),
  listen: vi.fn(),
  revealReport: vi.fn(),
  startScan: vi.fn()
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: mocks.listen
}));

vi.mock("./tauri", () => ({
  cancelScan: mocks.cancelScan,
  getScanReport: mocks.getScanReport,
  getScanStatus: mocks.getScanStatus,
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
  scanErrors: ["C:\\System Volume Information", "C:\\Windows\\Temp\\locked.tmp"],
  privacy: { uploaded: false },
  markdownReportPath: "D:\\Project\\report\\scan.md",
  jsonReportPath: "D:\\Project\\report\\scan.json",
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
    mocks.getScanReport.mockReset();
    mocks.getScanStatus.mockReset();
    mocks.listen.mockReset();
    mocks.revealReport.mockReset();
    mocks.startScan.mockReset();
    progressHandler = undefined;
  });

  it("shows the Chinese read-only safety ledger before scanning", () => {
    renderApp();

    expect(screen.getByRole("heading", { name: "C 盘空间诊断，先看清楚，再决定。" })).toBeInTheDocument();
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

    expect(screen.getByText("event progress")).toBeInTheDocument();
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
    expect(screen.getByText("event progress")).toBeInTheDocument();
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
    expect(screen.getByText("event complete")).toBeInTheDocument();
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

    expect(screen.getByText("扫描已排队。本次不会执行任何清理动作。")).toBeInTheDocument();
    expect(mocks.startScan).toHaveBeenCalledWith({ drive: "C", topCount: 30, largeFileMb: 200 });

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1200);
    });

    expect(screen.getByText("C:\\Users\\demo\\AppData\\Local\\Temp")).toBeInTheDocument();
    expect(screen.getByText("有 2 条路径因权限或系统保护无法读取，扫描已继续完成。")).toBeInTheDocument();
    expect(screen.getByLabelText("安全账本")).toHaveTextContent("3 个重解析点");

    fireEvent.click(screen.getByRole("button", { name: /系统托管项/ }));
    expect(screen.queryByText("C:\\Users\\demo\\AppData\\Local\\Temp")).not.toBeInTheDocument();
    expect(screen.getByText("C:\\Windows\\WinSxS")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /应用托管数据/ }));
    expect(screen.getByText("当前分组没有建议。")).toBeInTheDocument();
    expect(screen.queryByText("还没有加载报告。")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /显示 Markdown/ }));
    expect(mocks.revealReport).toHaveBeenCalledWith("scan-123", "markdown");

    vi.useRealTimers();
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

    expect(await screen.findByText("扫描已排队。本次不会执行任何清理动作。")).toBeInTheDocument();
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
    expect(screen.getByText("cancelled now")).toBeInTheDocument();

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
    expect(screen.getByText("cancelled now")).toBeInTheDocument();
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

    expect(await screen.findByText("扫描已排队。本次不会执行任何清理动作。")).toBeInTheDocument();
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
});

