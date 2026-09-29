//! 原生扫描内核（Phase 2 单层替换）：进程内 Rust 遍历。
//!
//! 输出与 PowerShell 扫描器（scripts/Scan-CDriveCleanupAdvisor.ps1）同构的
//! 原始报告（c-drive-cleanup-advisor-{timestamp}.md/.json），并遵守同一套
//! `[WCDCA_PROGRESS] {percent}|{code}` 进度协议。PowerShell 扫描器降级为
//! 可选兜底（WCDCA_SCAN_KERNEL=powershell 时启用），双内核等价性由
//! scripts/Test-ScannerContract.ps1 的 subst A/B 断言看护。
//!
//! 与 PS 内核的口径差异（显式文档，见 docs/release/KERNEL_MIGRATION.md）：
//! - 硬链接：两内核均按链接计数（稳定 std 无免费文件 ID，去重会改变 v0.2 报告数字）。
//! - scanErrors：本内核合并遍历，每个不可读路径只记录一次（PS 快扫会记录两次）。
//! - SizeGB 舍入：本内核 half-away-from-zero，PS 为 banker's rounding，仅 .xx5 中点不同。
//! - 排序平手：本内核按（SizeGB 降序, Path 升序）确定性排序；PS 平手次序不保证。
//! - pagefile/peak usage：本内核不经 WMI，只给出最佳可得信息（容量/休眠文件）。
//! - 扫描根直接重解析子项：两内核一致地静默跳过（不计入 SkippedReparsePoints）。

use crate::scanner::{
    decode_scanner_line, PROGRESS_CODE_DISM, PROGRESS_CODE_DRIVE_INFO, PROGRESS_CODE_DRILLDOWN,
    PROGRESS_CODE_DRILLDOWN_SCAN, PROGRESS_CODE_JSON, PROGRESS_CODE_LARGE_FILES,
    PROGRESS_CODE_LARGE_FILES_SCAN, PROGRESS_CODE_REPORT, PROGRESS_CODE_SYSTEM_INFO,
    PROGRESS_CODE_TOP_ROOTS, PROGRESS_CODE_TOP_ROOTS_SCAN, PROGRESS_MARKER_PREFIX,
    PROGRESS_MARKER_SEPARATOR, PROGRESS_PCT_DISM, PROGRESS_PCT_DRIVE_INFO, PROGRESS_PCT_DRILLDOWN_START,
    PROGRESS_PCT_JSON, PROGRESS_PCT_LARGE_FILES, PROGRESS_PCT_REPORT, PROGRESS_PCT_SYSTEM_INFO,
    PROGRESS_PCT_TOP_ROOTS, SCAN_STALL_TIMEOUT,
};
use serde_json::{json, Value};
use std::{
    collections::{HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// 内核选择环境变量；缺省为原生内核。
pub const SCAN_KERNEL_ENV: &str = "WCDCA_SCAN_KERNEL";
pub const SCAN_KERNEL_NATIVE: &str = "native";
pub const SCAN_KERNEL_POWERSHELL: &str = "powershell";

/// 心跳发射的最小间隔。
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(4);
/// 心跳百分比推进步长：每处理约 400 个目录推进 1%（与 PS 内核公式一致）。
const DIRS_PER_PERCENT: u64 = 400;
/// deep 模式深挖根的保留行数（与 PS 一致）。
const DRILLDOWN_ROW_LIMIT: usize = 20;
/// 大文件候选保留上限（与 PS 一致）。
const LARGE_FILE_ROW_LIMIT: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KernelError {
    /// 进度回调返回 false（用户取消）或任务已进入终态。
    Cancelled,
    Failed(String),
}

impl std::fmt::Display for KernelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KernelError::Cancelled => write!(f, "扫描已取消。"),
            KernelError::Failed(message) => write!(f, "{message}"),
        }
    }
}

impl From<std::io::Error> for KernelError {
    fn from(err: std::io::Error) -> Self {
        KernelError::Failed(err.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct KernelOptions {
    /// 扫描根的展示形（如 `C:\`）。
    pub drive_root: String,
    pub top_count: usize,
    /// 大文件阈值（字节）。
    pub large_file_threshold: u64,
    pub include_common_roots: bool,
    /// deep 模式深挖根列表（展示形路径）；为空时用默认清单（盘符参数化）。
    pub common_roots: Vec<String>,
    pub scan_error_limit: usize,
}

impl KernelOptions {
    pub fn new(
        drive_root: impl Into<String>,
        top_count: usize,
        large_file_mb: u64,
        include_common_roots: bool,
    ) -> Self {
        Self {
            drive_root: drive_root.into(),
            top_count,
            large_file_threshold: large_file_mb.saturating_mul(1024 * 1024),
            include_common_roots,
            common_roots: Vec::new(),
            scan_error_limit: 200,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DirRow {
    pub path: String,
    pub size_bytes: u64,
    pub files: u64,
    pub dirs: u64,
    pub skipped_reparse_points: u64,
}

#[derive(Debug, Clone)]
pub struct LargeFileRow {
    pub path: String,
    pub size_bytes: u64,
    /// Unix 毫秒；不可读时为 0。
    pub modified_unix_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ScanErrorRow {
    pub stage: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct SystemInfo {
    pub ram_gb: Option<f64>,
    pub pagefile_size_gb: Option<f64>,
    pub hibernation: Option<(String, f64)>,
}

#[derive(Debug, Clone)]
pub struct KernelScanResult {
    pub drive: String,
    pub is_elevated: bool,
    pub top: Vec<DirRow>,
    /// deep 模式深挖行（根展示路径 → 行）；quick 模式为空。
    pub drilldowns: Vec<(String, Vec<DirRow>)>,
    pub large_files: Vec<LargeFileRow>,
    pub system: SystemInfo,
    pub scan_errors: Vec<ScanErrorRow>,
    /// 截断前的错误总数（带上限截断而非静默丢弃）。
    pub scan_error_total: usize,
    pub scan_error_limit: usize,
    /// 是否运行过 DISM 组件存储分析（仅管理员）。
    pub dism_output: Option<String>,
}

/// 进度回调：`(percent, code_with_path)`；返回 false 表示立即取消扫描。
/// `code_with_path` 形如 `DRIVE_INFO` 或 `TOP_ROOTS_SCAN:C:\Users`，
/// 与 PS 发射端的标记负载完全一致。
pub type ProgressSink<'a> = dyn Fn(u8, &str) -> bool + Sync + 'a;

// ==== 路径与判定 ====

/// 将展示形路径转为 `\\?\` 打包形，支持超过 MAX_PATH 的长路径。
fn to_verbatim(display: &str) -> PathBuf {
    if display.starts_with(r"\\?\") {
        PathBuf::from(display)
    } else {
        PathBuf::from(format!(r"\\?\{}", display.trim_start_matches('\\')))
    }
}

fn join_display(parent: &str, name: &str) -> String {
    let parent = parent.trim_end_matches('\\');
    format!("{parent}\\{name}")
}

/// 进度码负载中的 `|` 会破坏标记协议，替换为 `/`（与 PS ConvertTo-ProgressCodeText 一致）。
fn sanitize_code_path(path: &str) -> String {
    path.replace('|', "/")
}

/// 判定元数据是否为重解析点（junction/符号链接/云占位等一律跳过，与 PS 一致）。
#[cfg(windows)]
fn metadata_is_reparse(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    meta.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
fn metadata_is_reparse(_meta: &fs::Metadata) -> bool {
    false
}

fn unix_ms(time: Option<SystemTime>) -> u64 {
    time.and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// SizeGB 舍入（2 位小数，half-away-from-zero；与 PS banker's rounding 的差异见模块注释）。
fn size_gb_rounded(bytes: u64) -> f64 {
    ((bytes as f64 / (1024.0 * 1024.0 * 1024.0)) * 100.0).round() / 100.0
}

/// 心跳百分比：与 PS 相同的 dirs/400 推进公式，封顶于区间末端。
fn heartbeat_percent(start: u8, end: u8, dirs_seen: u64) -> u8 {
    let span = end.saturating_sub(start) as u64;
    let offset = (dirs_seen / DIRS_PER_PERCENT).min(span);
    (start as u64 + offset).min(end as u64) as u8
}

/// 排序：精确字节降序，平手按路径升序——确定性次序。
/// PS 按舍入后 SizeGB 排序，舍入并列时次序不保证；该平手口径差异见模块注释。
fn sort_rows<T>(rows: &mut [T], size_of: impl Fn(&T) -> u64, path_of: impl Fn(&T) -> &str) {
    rows.sort_by(|a, b| {
        size_of(b)
            .cmp(&size_of(a))
            .then_with(|| path_of(a).cmp(path_of(b)))
    });
}

// ==== 遍历状态 ====

#[derive(Default, Clone)]
struct AnchorOutcome {
    size_bytes: u64,
    files: u64,
    dirs: u64,
    skipped: u64,
    large: Vec<LargeFileRow>,
}

#[derive(Default)]
struct WalkState {
    anchors: Mutex<Vec<(String, AnchorOutcome)>>,
    errors: Mutex<Vec<ScanErrorRow>>,
    error_total: AtomicU64,
    global_dirs: AtomicU64,
    cancel: AtomicBool,
}

impl WalkState {
    fn record_error(&self, stage: &str, path: &str, message: &str, limit: usize) {
        self.error_total.fetch_add(1, Ordering::Relaxed);
        if let Ok(mut locked) = self.errors.lock() {
            if locked.len() < limit {
                locked.push(ScanErrorRow {
                    stage: stage.to_string(),
                    path: path.to_string(),
                    message: message.to_string(),
                });
            }
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// 发射进度；回调返回 false 时置取消标记。
fn emit(state: &WalkState, progress: &ProgressSink<'_>, percent: u8, code_with_path: &str) {
    if !progress(percent, code_with_path) {
        state.cancel.store(true, Ordering::Relaxed);
    }
}

// ==== 树遍历 ====

/// 递归遍历一棵子树，返回累计体量/文件数/目录数/跳过数/大文件。
/// 遍历语义与 PS `Get-LocalTreeSize` 一致：目录计数含子树根；
/// 重解析点（含子树根被父级过滤）计入 SkippedReparsePoints；
/// 不可读路径入 scanErrors 不中止。
fn walk_subtree(
    root_display: &str,
    options: &KernelOptions,
    state: &WalkState,
    progress: &ProgressSink<'_>,
    percent_start: u8,
    percent_end: u8,
    heartbeat_code: &str,
) -> Result<AnchorOutcome, KernelError> {
    let mut outcome = AnchorOutcome::default();
    let mut stack: VecDeque<(PathBuf, String)> = VecDeque::new();
    stack.push_back((to_verbatim(root_display), root_display.to_string()));
    let mut last_beat = Instant::now();

    while let Some((dir_path, dir_display)) = stack.pop_back() {
        if state.is_cancelled() {
            return Err(KernelError::Cancelled);
        }
        let meta = match fs::symlink_metadata(&dir_path) {
            Ok(meta) => meta,
            Err(err) => {
                state.record_error(
                    "TreeSizeEnumerate",
                    &dir_display,
                    &err.to_string(),
                    options.scan_error_limit,
                );
                continue;
            }
        };
        if metadata_is_reparse(&meta) {
            outcome.skipped += 1;
            continue;
        }
        outcome.dirs += 1;
        let dirs_seen = state.global_dirs.fetch_add(1, Ordering::Relaxed) + 1;
        if last_beat.elapsed() >= HEARTBEAT_INTERVAL {
            emit(
                state,
                progress,
                heartbeat_percent(percent_start, percent_end, dirs_seen),
                &format!("{heartbeat_code}{}", sanitize_code_path(&dir_display)),
            );
            last_beat = Instant::now();
        }

        let entries = match fs::read_dir(&dir_path) {
            Ok(entries) => entries,
            Err(err) => {
                state.record_error(
                    "TreeSizeEnumerate",
                    &dir_display,
                    &err.to_string(),
                    options.scan_error_limit,
                );
                continue;
            }
        };

        for entry in entries {
            if state.is_cancelled() {
                return Err(KernelError::Cancelled);
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    state.record_error(
                        "TreeSizeEntry",
                        &dir_display,
                        &err.to_string(),
                        options.scan_error_limit,
                    );
                    continue;
                }
            };
            // DirEntry::metadata 在 Windows 上复用目录枚举的 find-data 缓存，无额外系统调用。
            let entry_meta = match entry.metadata() {
                Ok(meta) => meta,
                Err(err) => {
                    state.record_error(
                        "TreeSizeEntry",
                        &dir_display,
                        &err.to_string(),
                        options.scan_error_limit,
                    );
                    continue;
                }
            };
            if metadata_is_reparse(&entry_meta) {
                outcome.skipped += 1;
                continue;
            }
            let entry_display = join_display(&dir_display, &entry.file_name().to_string_lossy());
            if entry_meta.is_dir() {
                stack.push_back((entry.path(), entry_display));
            } else {
                let size = entry_meta.len();
                outcome.size_bytes = outcome.size_bytes.saturating_add(size);
                outcome.files += 1;
                if size >= options.large_file_threshold {
                    outcome.large.push(LargeFileRow {
                        path: entry_display,
                        size_bytes: size,
                        modified_unix_ms: unix_ms(entry_meta.modified().ok()),
                    });
                }
            }
        }
    }

    Ok(outcome)
}

/// 枚举 root 的直接子项并生成报告行（语义与 PS `Get-ChildSizeReport` 一致）：
/// 目录走完整子树、文件成单行；直接重解析子项静默跳过；按 SizeGB 降序取前 count 行。
#[allow(clippy::too_many_arguments)]
fn collect_child_rows(
    root: &str,
    count: usize,
    options: &KernelOptions,
    state: &WalkState,
    progress: &ProgressSink<'_>,
    percent_start: u8,
    percent_end: u8,
    heartbeat_code: &str,
) -> Result<Vec<DirRow>, KernelError> {
    let mut rows: Vec<DirRow> = Vec::new();
    let entries = match fs::read_dir(to_verbatim(root)) {
        Ok(entries) => entries,
        Err(err) => {
            state.record_error(
                "TreeSizeEnumerate",
                root,
                &err.to_string(),
                options.scan_error_limit,
            );
            return Ok(rows);
        }
    };
    for entry in entries.flatten() {
        if state.is_cancelled() {
            return Err(KernelError::Cancelled);
        }
        let Ok(entry_meta) = entry.metadata() else { continue };
        if metadata_is_reparse(&entry_meta) {
            continue;
        }
        let display = join_display(root, &entry.file_name().to_string_lossy());
        // 与 PS Get-ChildSizeReport 一致：每个子项处理前发射一次心跳（小树也可见进度）。
        emit(
            state,
            progress,
            heartbeat_percent(percent_start, percent_end, state.global_dirs.load(Ordering::Relaxed)),
            &format!("{heartbeat_code}{}", sanitize_code_path(&display)),
        );
        if entry_meta.is_dir() {
            let outcome = walk_subtree(
                &display,
                options,
                state,
                progress,
                percent_start,
                percent_end,
                heartbeat_code,
            )?;
            rows.push(dir_row(&display, outcome));
        } else {
            let size = entry_meta.len();
            let mut outcome = AnchorOutcome::default();
            outcome.size_bytes = size;
            outcome.files = 1;
            rows.push(dir_row(&display, outcome));
        }
    }
    sort_rows(&mut rows, |row| row.size_bytes, |row| row.path.as_str());
    rows.truncate(count);
    Ok(rows)
}

fn dir_row(display: &str, outcome: AnchorOutcome) -> DirRow {
    DirRow {
        path: display.to_string(),
        size_bytes: outcome.size_bytes,
        files: outcome.files,
        dirs: outcome.dirs,
        skipped_reparse_points: outcome.skipped,
    }
}

// ==== 系统信息与提权检测 ====

#[cfg(windows)]
fn is_process_elevated() -> bool {
    use std::mem;
    unsafe {
        let mut token: *mut std::ffi::c_void = std::ptr::null_mut();
        let ok = windows_sys::Win32::System::Threading::OpenProcessToken(
            windows_sys::Win32::System::Threading::GetCurrentProcess(),
            windows_sys::Win32::Security::TOKEN_QUERY,
            &mut token,
        );
        if ok == 0 {
            return false;
        }
        let mut elevation: windows_sys::Win32::Security::TOKEN_ELEVATION = mem::zeroed();
        let mut returned: u32 = 0;
        let ok = windows_sys::Win32::Security::GetTokenInformation(
            token,
            windows_sys::Win32::Security::TokenElevation,
            &mut elevation as *mut _ as *mut _,
            mem::size_of::<windows_sys::Win32::Security::TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        windows_sys::Win32::Foundation::CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

#[cfg(not(windows))]
fn is_process_elevated() -> bool {
    false
}

#[cfg(windows)]
fn read_system_info(drive_root: &str) -> SystemInfo {
    use std::mem;
    let mut info = SystemInfo::default();
    unsafe {
        let mut status: windows_sys::Win32::System::SystemInformation::MEMORYSTATUSEX = mem::zeroed();
        status.dwLength =
            mem::size_of::<windows_sys::Win32::System::SystemInformation::MEMORYSTATUSEX>() as u32;
        if windows_sys::Win32::System::SystemInformation::GlobalMemoryStatusEx(&mut status) != 0 {
            info.ram_gb = Some(
                (status.ullTotalPhys as f64 / (1024.0 * 1024.0 * 1024.0) * 100.0).round() / 100.0,
            );
        }
    }
    let pagefile = join_display(drive_root, "pagefile.sys");
    if let Ok(meta) = fs::symlink_metadata(to_verbatim(&pagefile)) {
        info.pagefile_size_gb = Some(size_gb_rounded(meta.len()));
    }
    let hiberfil = join_display(drive_root, "hiberfil.sys");
    if let Ok(meta) = fs::symlink_metadata(to_verbatim(&hiberfil)) {
        info.hibernation = Some((hiberfil, size_gb_rounded(meta.len())));
    }
    info
}

#[cfg(not(windows))]
fn read_system_info(_drive_root: &str) -> SystemInfo {
    SystemInfo::default()
}

/// DISM 组件存储分析（只读分析命令）；仅管理员运行时执行，超时或取消不中止扫描。
fn run_dism_analysis(state: &WalkState) -> Option<String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new("Dism.exe")
        .args(["/Online", "/Cleanup-Image", "/AnalyzeComponentStore"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    // DISM 阶段结构性静默（AGENT.md 已知限制）：按总超时保底，取消信号优先。
    let deadline = Instant::now() + SCAN_STALL_TIMEOUT;
    loop {
        if state.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                thread::sleep(Duration::from_millis(200));
            }
            Err(_) => return None,
        }
    }
    let mut output = Vec::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_end(&mut output);
    }
    // Dism 输出走控制台 OEM 码页：复用扫描器行解码（UTF-8 优先，GBK 兜底）。
    let text = decode_scanner_line(&output);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

// ==== 主入口 ====

/// 执行一次只读扫描。进度协议与 PS 内核一致：
/// DRIVE_INFO@12 → TOP_ROOTS@18 →（遍历心跳 TOP_ROOTS_SCAN）
/// →（deep：DRILLDOWN/DRILLDOWN_SCAN）→ LARGE_FILES@65 → SYSTEM_INFO@74
/// →（管理员：DISM@82）→ 结束；REPORT/JSON 标记由 `write_raw_reports` 发射。
pub fn run_scan(
    options: &KernelOptions,
    progress: &ProgressSink<'_>,
) -> Result<KernelScanResult, KernelError> {
    let state = WalkState::default();
    let drive_root = options.drive_root.trim_end_matches('\\').to_string() + "\\";

    emit(&state, progress, PROGRESS_PCT_DRIVE_INFO, PROGRESS_CODE_DRIVE_INFO);
    let is_elevated = is_process_elevated();

    emit(&state, progress, PROGRESS_PCT_TOP_ROOTS, PROGRESS_CODE_TOP_ROOTS);

    // ==== 阶段 1：单次全盘遍历（替代 PS 的顶层遍历 + 大文件二次遍历）====
    // 心跳码 TOP_ROOTS_SCAN，百分比区间 quick 18..62 / deep 18..34。
    let top_end: u8 = if options.include_common_roots {
        PROGRESS_PCT_DRILLDOWN_START - 1
    } else {
        62
    };

    let mut root_file_rows: Vec<DirRow> = Vec::new();
    let mut root_large: Vec<LargeFileRow> = Vec::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    match fs::read_dir(to_verbatim(&drive_root)) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let Ok(entry_meta) = entry.metadata() else { continue };
                if metadata_is_reparse(&entry_meta) {
                    // 与 PS Get-ChildSizeReport 一致：扫描根直接重解析子项静默跳过。
                    continue;
                }
                let display = join_display(&drive_root, &entry.file_name().to_string_lossy());
                if entry_meta.is_dir() {
                    queue.push_back(display);
                } else {
                    let size = entry_meta.len();
                    let mut outcome = AnchorOutcome::default();
                    outcome.size_bytes = size;
                    outcome.files = 1;
                    if size >= options.large_file_threshold {
                        // 根级文件同样参与大文件候选（PS 大文件遍历可见它们）。
                        root_large.push(LargeFileRow {
                            path: display.clone(),
                            size_bytes: size,
                            modified_unix_ms: unix_ms(entry_meta.modified().ok()),
                        });
                    }
                    root_file_rows.push(dir_row(&display, outcome));
                }
            }
        }
        Err(err) => {
            return Err(KernelError::Failed(format!(
                "无法枚举扫描根 {drive_root}：{err}"
            )));
        }
    }

    // 并行遍历各顶层子树：共享工作队列，工作线程数不超过可用并行度。
    let queue = Arc::new(Mutex::new(queue));
    let worker_count = thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(4)
        .max(1);
    thread::scope(|scope| {
        for _ in 0..worker_count {
            let queue = Arc::clone(&queue);
            let state = &state;
            let options = options;
            let progress = progress;
            scope.spawn(move || loop {
                if state.is_cancelled() {
                    return;
                }
                let next = queue.lock().ok().and_then(|mut locked| locked.pop_front());
                let Some(display) = next else { return };
                // 锚点起步即发射一次心跳（与 PS 逐顶层子项心跳一致），保证小树也可见进度。
                let dirs_seen = state.global_dirs.load(Ordering::Relaxed);
                emit(
                    state,
                    progress,
                    heartbeat_percent(PROGRESS_PCT_TOP_ROOTS, top_end, dirs_seen),
                    &format!(
                        "{PROGRESS_CODE_TOP_ROOTS_SCAN}{}",
                        sanitize_code_path(&display)
                    ),
                );
                match walk_subtree(
                    &display,
                    options,
                    state,
                    progress,
                    PROGRESS_PCT_TOP_ROOTS,
                    top_end,
                    PROGRESS_CODE_TOP_ROOTS_SCAN,
                ) {
                    Ok(outcome) => {
                        if let Ok(mut locked) = state.anchors.lock() {
                            locked.push((display, outcome));
                        }
                    }
                    Err(KernelError::Cancelled) => {
                        state.cancel.store(true, Ordering::Relaxed);
                        return;
                    }
                    Err(KernelError::Failed(_)) => return,
                }
            });
        }
    });

    if state.is_cancelled() {
        return Err(KernelError::Cancelled);
    }

    // ==== 阶段 2（deep）：重点目录深挖 ====
    let mut drilldowns: Vec<(String, Vec<DirRow>)> = Vec::new();
    if options.include_common_roots {
        let roots: Vec<String> = if options.common_roots.is_empty() {
            default_common_roots(&drive_root)
        } else {
            options.common_roots.clone()
        };
        let roots: Vec<String> = roots
            .into_iter()
            .filter(|root| to_verbatim(root).exists())
            .collect();
        let total = roots.len().max(1);
        for (index, root) in roots.iter().enumerate() {
            if state.is_cancelled() {
                return Err(KernelError::Cancelled);
            }
            let drill_percent =
                (PROGRESS_PCT_DRILLDOWN_START as usize + (index * 25) / total).min(60) as u8;
            emit(
                &state,
                progress,
                drill_percent,
                &format!(
                    "{PROGRESS_CODE_DRILLDOWN}{}",
                    sanitize_code_path(root)
                ),
            );
            let drill_end = (drill_percent + 2).min(62);
            let rows = collect_child_rows(
                root,
                DRILLDOWN_ROW_LIMIT,
                options,
                &state,
                progress,
                drill_percent,
                drill_end,
                PROGRESS_CODE_DRILLDOWN_SCAN,
            )?;
            drilldowns.push((root.clone(), rows));
        }
    }

    // ==== 大文件候选收尾（单次遍历已收集；发射 LARGE_FILES 标记保持协议形状）====
    emit(&state, progress, PROGRESS_PCT_LARGE_FILES, PROGRESS_CODE_LARGE_FILES);
    emit(
        &state,
        progress,
        PROGRESS_PCT_LARGE_FILES,
        &format!(
            "{PROGRESS_CODE_LARGE_FILES_SCAN}{}",
            sanitize_code_path(&drive_root)
        ),
    );

    // ==== 阶段 3：系统信息 / DISM ====
    emit(
        &state,
        progress,
        PROGRESS_PCT_SYSTEM_INFO,
        PROGRESS_CODE_SYSTEM_INFO,
    );
    let system = read_system_info(&drive_root);

    let dism_output = if is_elevated {
        emit(&state, progress, PROGRESS_PCT_DISM, PROGRESS_CODE_DISM);
        run_dism_analysis(&state)
    } else {
        None
    };

    if state.is_cancelled() {
        return Err(KernelError::Cancelled);
    }

    // ==== 汇总 ====
    let mut top: Vec<DirRow> = root_file_rows;
    let mut large_files: Vec<LargeFileRow> = root_large;
    {
        let anchors = state.anchors.lock().map_err(|_| {
            KernelError::Failed("扫描状态暂不可用。".to_string())
        })?;
        for (display, outcome) in anchors.iter() {
            top.push(dir_row(display, outcome.clone()));
            large_files.extend(outcome.large.iter().cloned());
        }
    }
    sort_rows(&mut top, |row| row.size_bytes, |row| row.path.as_str());
    top.truncate(options.top_count.max(1));

    // 大文件全局去重（deep 深挖可能重访同一文件）后排序截断。
    let mut seen_large: HashSet<String> = HashSet::new();
    large_files.retain(|row| seen_large.insert(row.path.to_ascii_lowercase()));
    sort_rows(
        &mut large_files,
        |row| row.size_bytes,
        |row| row.path.as_str(),
    );
    large_files.truncate(LARGE_FILE_ROW_LIMIT);

    let (scan_errors, error_total) = {
        let errors = state.errors.lock().map_err(|_| {
            KernelError::Failed("扫描状态暂不可用。".to_string())
        })?;
        (errors.clone(), state.error_total.load(Ordering::Relaxed) as usize)
    };

    Ok(KernelScanResult {
        drive: drive_root,
        is_elevated,
        top,
        drilldowns,
        large_files,
        system,
        scan_errors,
        scan_error_total: error_total,
        scan_error_limit: options.scan_error_limit,
        dism_output,
    })
}

/// deep 模式默认深挖根（与 PS $commonRoots 对齐；盘符参数化为本次扫描盘）。
fn default_common_roots(drive_root: &str) -> Vec<String> {
    let user_profile = std::env::var("USERPROFILE").unwrap_or_default();
    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
    let app_data = std::env::var("APPDATA").unwrap_or_default();
    let mut roots = Vec::new();
    let mut push = |value: String| {
        if !value.is_empty() && !roots.contains(&value) {
            roots.push(value);
        }
    };
    push(user_profile.clone());
    push(join_display(&local_app_data, "NVIDIA"));
    push(join_display(&local_app_data, "Microsoft"));
    push(join_display(&local_app_data, "JianyingPro"));
    push(join_display(&local_app_data, "Packages"));
    push(join_display(&local_app_data, "Programs"));
    push(join_display(&app_data, "Tencent"));
    push(join_display(&app_data, "kingsoft"));
    push(join_display(&app_data, "Code"));
    push(join_display(&user_profile, ".cache"));
    push(join_display(&user_profile, ".vscode"));
    push(join_display(&user_profile, "Downloads"));
    push(join_display(&user_profile, "Desktop"));
    push(join_display(drive_root, "ProgramData"));
    push(join_display(drive_root, "Windows"));
    roots
}

// ==== 原始报告输出（与 PS 内核同构）====

fn dir_row_json(row: &DirRow) -> Value {
    json!({
        "Path": row.path,
        "SizeGB": size_gb_rounded(row.size_bytes),
        "Files": row.files,
        "Dirs": row.dirs,
        "SkippedReparsePoints": row.skipped_reparse_points,
    })
}

fn large_file_json(row: &LargeFileRow) -> Value {
    let modified = if row.modified_unix_ms == 0 {
        Value::Null
    } else {
        json!(row.modified_unix_ms)
    };
    json!({
        "SizeGB": size_gb_rounded(row.size_bytes),
        "LastWriteTime": modified,
        "Path": row.path,
    })
}

/// 构造与 PS `ConvertTo-Json` 同构的原始 JSON 值（字段名保持 PS 属性名）。
pub fn raw_json_value(result: &KernelScanResult) -> Value {
    let mut drilldowns = serde_json::Map::new();
    for (root, rows) in &result.drilldowns {
        drilldowns.insert(
            root.clone(),
            Value::Array(rows.iter().map(dir_row_json).collect()),
        );
    }

    let pagefile: Vec<Value> = result
        .system
        .pagefile_size_gb
        .map(|size_gb| {
            vec![json!({
                "Name": join_display(&result.drive, "pagefile.sys"),
                "AllocatedBaseSize": (size_gb * 1024.0).round(),
                "CurrentUsage": Value::Null,
                "PeakUsage": Value::Null,
            })]
        })
        .unwrap_or_default();

    let hibernation = result.system.hibernation.as_ref().map(|(path, size_gb)| {
        json!({
            "FullName": path,
            "SizeGB": size_gb,
        })
    });

    json!({
        "generated": chrono::Local::now().to_rfc3339(),
        "drive": result.drive,
        "isAdmin": result.is_elevated,
        "top": Value::Array(result.top.iter().map(dir_row_json).collect()),
        "drilldowns": Value::Object(drilldowns),
        "largeFiles": Value::Array(result.large_files.iter().map(large_file_json).collect()),
        "pagefile": Value::Array(pagefile),
        "computer": {
            "AutomaticManagedPagefile": Value::Null,
            "RAMGB": result.system.ram_gb,
        },
        "hibernation": hibernation,
        "scanErrors": Value::Array(result.scan_errors.iter().map(|error| {
            json!({
                "Stage": error.stage,
                "Path": error.path,
                "Message": error.message,
            })
        }).collect()),
        "scanErrorTotal": result.scan_error_total,
        "scanErrorLimit": result.scan_error_limit,
    })
}

fn md_table(builder: &mut String, title: &str, rows: &[DirRow], large: &[LargeFileRow]) {
    builder.push('\n');
    builder.push_str(&format!("## {title}\n"));
    builder.push('\n');
    if rows.is_empty() && large.is_empty() {
        builder.push_str("No data.\n");
        return;
    }
    builder.push_str("| Size GB | Path | Notes |\n");
    builder.push_str("|---:|---|---|\n");
    for row in rows {
        let path = sanitize_code_path(&row.path);
        builder.push_str(&format!(
            "| {} | `{}` | files={}, dirs={} |\n",
            size_gb_rounded(row.size_bytes),
            path,
            row.files,
            row.dirs
        ));
    }
    for row in large {
        let path = sanitize_code_path(&row.path);
        builder.push_str(&format!(
            "| {} | `{}` | modified={} |\n",
            size_gb_rounded(row.size_bytes),
            path,
            row.modified_unix_ms
        ));
    }
}

/// 渲染与 PS 内核同构的 Markdown 报告（冻结文案逐条保留）。
pub fn render_markdown(result: &KernelScanResult) -> String {
    let mut builder = String::new();
    builder.push_str("# Windows C Drive Cleanup Advisor Report\n\n");
    builder.push_str(&format!(
        "Generated: {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    ));
    builder.push_str("Read-only scan: yes\n");
    builder.push_str("Skipped reparse points: yes\n");
    builder.push_str(&format!(
        "Running as administrator: {}\n\n",
        if result.is_elevated { "True" } else { "False" }
    ));

    builder.push_str(&format!("Drive {} total: unknown GB\n", result.drive));
    builder.push_str(&format!("Drive {} free: unknown GB\n", result.drive));
    builder.push_str(&format!(
        "Drive {} free percent: unknown%\n",
        result.drive
    ));

    md_table(&mut builder, "Top Real C Drive Roots", &result.top, &[]);
    for (root, rows) in &result.drilldowns {
        md_table(&mut builder, &format!("Drilldown: {root}"), rows, &[]);
    }
    md_table(&mut builder, "Large Files", &[], &result.large_files);

    builder.push_str("\n## Scan Notes\n\n");
    if result.scan_errors.is_empty() {
        builder.push_str("- No scan access errors were recorded.\n");
    } else {
        builder.push_str(
            "- Some paths could not be read. This is common for protected Windows or app-managed folders.\n",
        );
        for error in result.scan_errors.iter().take(20) {
            let path = sanitize_code_path(&error.path);
            let message = sanitize_code_path(&error.message);
            builder.push_str(&format!("- {}: `{}` - {}\n", error.stage, path, message));
        }
        if result.scan_error_total > 20 {
            builder.push_str(&format!(
                "- Additional scan errors omitted from Markdown: {}\n",
                result.scan_error_total - 20
            ));
        }
    }

    builder.push_str("\n## System Managed Items\n\n### Pagefile\n\n");
    if let Some(size_gb) = result.system.pagefile_size_gb {
        builder.push_str(&format!(
            "- {}: size={} GB (native kernel measures the pagefile directly)\n",
            join_display(&result.drive, "pagefile.sys"),
            size_gb
        ));
    } else {
        builder.push_str("- Pagefile: not readable from this process.\n");
    }
    if let Some(ram_gb) = result.system.ram_gb {
        builder.push_str(&format!("- RAM: {ram_gb} GB\n"));
    }
    builder.push_str("- Automatic managed pagefile: unknown (native kernel does not query WMI)\n");
    match &result.system.hibernation {
        Some((path, size_gb)) => {
            builder.push_str(&format!("- Hibernation file: {size_gb} GB at {path}\n"));
        }
        None => builder.push_str("- Hibernation file: not found\n"),
    }

    builder.push_str("\n### WinSxS\n\n");
    match &result.dism_output {
        Some(output) => {
            builder.push_str("```text\n");
            builder.push_str(output);
            builder.push_str("\n```\n");
        }
        None => {
            builder.push_str("Run as administrator for DISM AnalyzeComponentStore output:\n\n");
            builder.push_str("```powershell\n");
            builder.push_str("Dism.exe /Online /Cleanup-Image /AnalyzeComponentStore\n");
            builder.push_str("```\n");
        }
    }

    builder.push_str("\n## Cleanup Advice\n\n");
    builder.push_str(
        "- Do not delete anything from WinSxS, Windows Installer, System32, or System Volume Information manually.\n",
    );
    builder.push_str(
        "- Prefer built-in app storage managers for WeChat, QQ, WXWork, WPS, and cloud drives.\n",
    );
    builder.push_str(
        "- GPU shader caches, browser caches, npm/pip/Gradle/Playwright caches, editor installer caches, and app update packages are usually good first-pass cleanup candidates.\n",
    );
    builder.push_str(
        "- Pagefile changes should be made through Windows virtual memory settings unless the user explicitly wants automation.\n",
    );
    builder.push_str("- This tool did not delete, move, or modify files.\n");
    builder
}

/// 写出原始 Markdown + JSON（文件名规则与 PS 内核一致），并发射 REPORT/JSON 进度标记。
pub fn write_raw_reports(
    result: &KernelScanResult,
    output_dir: &Path,
    progress: &ProgressSink<'_>,
) -> Result<(PathBuf, PathBuf), KernelError> {
    fs::create_dir_all(output_dir)?;
    let timestamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let markdown_path = output_dir.join(format!("c-drive-cleanup-advisor-{timestamp}.md"));
    let json_path = output_dir.join(format!("c-drive-cleanup-advisor-{timestamp}.json"));

    progress(PROGRESS_PCT_REPORT, PROGRESS_CODE_REPORT);
    fs::write(&markdown_path, render_markdown(result))?;
    progress(PROGRESS_PCT_JSON, PROGRESS_CODE_JSON);
    let json_text = serde_json::to_string_pretty(&raw_json_value(result))
        .map_err(|err| KernelError::Failed(err.to_string()))?;
    fs::write(&json_path, json_text)?;
    Ok((markdown_path, json_path))
}

// 供 wcdca-scan 测试二进制与契约脚本复用的标记行格式化。
pub fn format_marker_line(percent: u8, code_with_path: &str) -> String {
    format!(
        "{PROGRESS_MARKER_PREFIX}{percent}{PROGRESS_MARKER_SEPARATOR}{code_with_path}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use uuid::Uuid;

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("wcdca-kernel-{name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn display(path: &Path) -> String {
        // 测试里用非 verbatim 展示形。
        path.display().to_string()
    }

    fn write_file(path: &Path, bytes: usize) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let file = fs::File::create(path).unwrap();
        file.set_len(bytes as u64).unwrap();
    }

    fn quiet_progress() -> impl Fn(u8, &str) -> bool + Sync {
        |_, _| true
    }

    #[test]
    fn walk_counts_sizes_files_dirs() {
        let root = temp_root("sizes");
        write_file(&root.join("a\\one.bin"), 1024);
        write_file(&root.join("a\\two.bin"), 2048);
        write_file(&root.join("b\\deep\\three.bin"), 4096);

        let mut options = KernelOptions::new(display(&root), 30, 1, false);
        options.drive_root = display(&root);
        let result = run_scan(&options, &quiet_progress()).unwrap();

        assert_eq!(result.top.len(), 2);
        let a = result.top.iter().find(|row| row.path.ends_with("\\a")).unwrap();
        let b = result.top.iter().find(|row| row.path.ends_with("\\b")).unwrap();
        assert_eq!(a.size_bytes, 1024 + 2048);
        assert_eq!(a.files, 2);
        assert_eq!(a.dirs, 1, "目录计数含子树根本身（与 PS Get-LocalTreeSize 一致）");
        assert_eq!(b.size_bytes, 4096);
        assert_eq!(b.files, 1);
        assert_eq!(b.dirs, 2, "b 与 b\\deep 两层目录");
        assert_eq!(result.scan_errors.len(), 0);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn root_level_files_become_their_own_rows() {
        let root = temp_root("root-files");
        write_file(&root.join("big.bin"), 3 * 1024 * 1024);
        write_file(&root.join("sub\\small.bin"), 10);

        let mut options = KernelOptions::new(display(&root), 30, 1, false);
        options.drive_root = display(&root);
        let result = run_scan(&options, &quiet_progress()).unwrap();

        assert!(result
            .top
            .iter()
            .any(|row| row.path.ends_with("big.bin") && row.size_bytes == 3 * 1024 * 1024));
        assert!(result.top.iter().any(|row| row.path.ends_with("\\sub")));
        assert!(
            result
                .large_files
                .iter()
                .any(|row| row.path.ends_with("big.bin")),
            "根级文件也参与大文件候选"
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn large_files_respect_threshold_and_limit() {
        let root = temp_root("large");
        write_file(&root.join("a.bin"), 2 * 1024 * 1024);
        write_file(&root.join("b.bin"), 1024 * 1024);
        write_file(&root.join("c.bin"), 512);

        let mut options = KernelOptions::new(display(&root), 30, 1, false);
        options.drive_root = display(&root);
        let result = run_scan(&options, &quiet_progress()).unwrap();

        let names: Vec<String> = result
            .large_files
            .iter()
            .map(|row| row.path.rsplit('\\').next().unwrap().to_string())
            .collect();
        assert_eq!(names, vec!["a.bin".to_string(), "b.bin".to_string()]);
        assert_eq!(result.large_files[0].size_bytes, 2 * 1024 * 1024);
        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn reparse_points_are_skipped_and_counted() {
        let root = temp_root("reparse");
        write_file(&root.join("CacheRoot\\data.bin"), 2048);
        write_file(&root.join("target\\inside.bin"), 1024);
        let link = root.join("CacheRoot\\link");
        // junction 不需要管理员权限；失败则跳过断言（受限环境）。
        let made = std::process::Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                &display(&link),
                &display(&root.join("target")),
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if !made {
            fs::remove_dir_all(&root).unwrap();
            return;
        }

        let mut options = KernelOptions::new(display(&root), 30, 1, false);
        options.drive_root = display(&root);
        let result = run_scan(&options, &quiet_progress()).unwrap();

        let cache = result
            .top
            .iter()
            .find(|row| row.path.ends_with("CacheRoot"))
            .unwrap();
        assert_eq!(
            cache.size_bytes, 2048,
            "junction 目标不计入体量（不重复计算）"
        );
        assert_eq!(cache.skipped_reparse_points, 1, "junction 计入跳过数");
        assert_eq!(result.top.iter().find(|row| row.path.ends_with("\\target")).unwrap().size_bytes, 1024);
        fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn hardlinks_are_counted_per_link_matching_ps_parity() {
        let root = temp_root("hardlink");
        let original = root.join("sub\\a.bin");
        write_file(&original, 4096);
        fs::hard_link(&original, root.join("sub\\b.bin")).unwrap();

        let mut options = KernelOptions::new(display(&root), 30, 1, false);
        options.drive_root = display(&root);
        let result = run_scan(&options, &quiet_progress()).unwrap();

        let sub = result.top.iter().find(|row| row.path.ends_with("\\sub")).unwrap();
        assert_eq!(
            sub.size_bytes,
            8192,
            "与 PS 口径一致：硬链接按链接计数（差异已文档化）"
        );
        assert_eq!(sub.files, 2);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn unreadable_paths_record_errors_and_do_not_abort() {
        let root = temp_root("errors");
        write_file(&root.join("ok\\fine.bin"), 1024);
        // 用“文件冒充深挖根”触发可复现的读取失败（read_dir(文件) 必然失败），
        // 与 ACL 拒绝同形：入 scanErrors、不中止、其余结果照常产出。
        let fake_root = root.join("not-a-dir.bin");
        write_file(&fake_root, 16);
        let mut options = KernelOptions::new(display(&root), 30, 1, true);
        options.drive_root = display(&root);
        options.common_roots = vec![display(&fake_root)];
        let result = run_scan(&options, &quiet_progress()).unwrap();

        assert!(result.scan_error_total >= 1, "不可读路径必须计入 scanErrors");
        assert!(result.scan_errors[0].stage.starts_with("TreeSize"));
        assert!(result.scan_errors[0].path.contains("not-a-dir.bin"));
        assert!(result.top.iter().any(|row| row.path.ends_with("ok")));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn scan_error_limit_keeps_total_count() {
        let state = WalkState::default();
        for index in 0..25 {
            state.record_error("TreeSizeEnumerate", &format!("path-{index}"), "denied", 5);
        }
        assert_eq!(state.errors.lock().unwrap().len(), 5);
        assert_eq!(state.error_total.load(Ordering::Relaxed), 25);
    }

    #[test]
    fn progress_markers_follow_frozen_stage_order() {
        let root = temp_root("progress");
        write_file(&root.join("a\\one.bin"), 2048);

        let seen: Arc<Mutex<Vec<(u8, String)>>> = Arc::new(Mutex::new(Vec::new()));
        let seen_in = Arc::clone(&seen);
        let mut options = KernelOptions::new(display(&root), 30, 1, true);
        options.drive_root = display(&root);
        options.common_roots = vec![display(&root.join("a"))];
        run_scan(&options, &move |percent, code| {
            seen_in.lock().unwrap().push((percent, code.to_string()));
            true
        })
        .unwrap();

        let seen = seen.lock().unwrap();
        let codes: Vec<&str> = seen.iter().map(|(_, code)| code.split(':').next().unwrap()).collect();
        assert_eq!(codes[0], "DRIVE_INFO");
        assert!(codes.contains(&"TOP_ROOTS"));
        assert!(
            codes.contains(&"TOP_ROOTS_SCAN"),
            "顶层心跳必须出现（小树也有锚点起步心跳）"
        );
        assert!(codes.contains(&"DRILLDOWN"));
        assert!(codes.contains(&"DRILLDOWN_SCAN"));
        assert!(codes.contains(&"LARGE_FILES"));
        assert!(codes.contains(&"LARGE_FILES_SCAN"));
        assert!(codes.contains(&"SYSTEM_INFO"));
        // 固定码次序：DRIVE_INFO → TOP_ROOTS → ... → LARGE_FILES → SYSTEM_INFO
        let pos = |needle: &str| codes.iter().position(|code| *code == needle).unwrap();
        assert!(pos("DRIVE_INFO") < pos("TOP_ROOTS"));
        assert!(pos("TOP_ROOTS") < pos("LARGE_FILES"));
        assert!(pos("LARGE_FILES") < pos("SYSTEM_INFO"));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn cancellation_through_progress_callback_stops_scan() {
        let root = temp_root("cancel");
        for index in 0..8 {
            write_file(&root.join(format!("dir-{index}\\f.bin")), 1024);
        }
        let mut options = KernelOptions::new(display(&root), 30, 1, false);
        options.drive_root = display(&root);
        let result = run_scan(&options, &|percent, _| percent < PROGRESS_PCT_TOP_ROOTS);
        assert_eq!(result.unwrap_err(), KernelError::Cancelled);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn raw_json_preserves_ps_property_names() {
        let root = temp_root("json");
        write_file(&root.join("a\\one.bin"), 1024 * 1024);
        let mut options = KernelOptions::new(display(&root), 30, 1, false);
        options.drive_root = display(&root);
        let result = run_scan(&options, &quiet_progress()).unwrap();
        let value = raw_json_value(&result);

        assert_eq!(value.get("drive").and_then(Value::as_str), Some(result.drive.as_str()));
        assert!(value.get("top").and_then(Value::as_array).is_some());
        assert!(value.get("drilldowns").and_then(Value::as_object).is_some());
        assert!(value.get("largeFiles").and_then(Value::as_array).is_some());
        assert!(value.get("scanErrors").and_then(Value::as_array).is_some());
        assert!(value.get("scanErrorLimit").and_then(Value::as_u64).is_some());
        let row = &value.get("top").unwrap().as_array().unwrap()[0];
        assert!(row.get("Path").is_some());
        assert!(row.get("SizeGB").is_some());
        assert!(row.get("Files").is_some());
        assert!(row.get("Dirs").is_some());
        assert!(row.get("SkippedReparsePoints").is_some());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn markdown_contains_frozen_statements() {
        let root = temp_root("markdown");
        write_file(&root.join("a\\one.bin"), 1024);
        let mut options = KernelOptions::new(display(&root), 30, 1, false);
        options.drive_root = display(&root);
        let result = run_scan(&options, &quiet_progress()).unwrap();
        let markdown = render_markdown(&result);

        assert!(markdown.contains("Read-only scan: yes"));
        assert!(markdown.contains("This tool did not delete, move, or modify files."));
        assert!(markdown.contains("## Top Real C Drive Roots"));
        assert!(markdown.contains("## Cleanup Advice"));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn deep_mode_drilldown_rows_are_capped_and_sorted() {
        let root = temp_root("drill");
        for index in 0..25 {
            write_file(
                &root.join(format!("root\\child-{index:02}\\f.bin")),
                (index + 1) * 1024,
            );
        }
        let mut options = KernelOptions::new(display(&root), 30, 1, true);
        options.drive_root = display(&root);
        options.common_roots = vec![display(&root.join("root"))];
        let result = run_scan(&options, &quiet_progress()).unwrap();

        assert_eq!(result.drilldowns.len(), 1);
        let rows = &result.drilldowns[0].1;
        assert_eq!(rows.len(), 20, "深挖行上限 20（与 PS 一致）");
        assert!(
            rows[0].size_bytes >= rows[1].size_bytes,
            "深挖行按体量降序"
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn format_marker_line_matches_protocol_shape() {
        let line = format_marker_line(18, "TOP_ROOTS_SCAN:C:\\Users");
        assert_eq!(line, "[WCDCA_PROGRESS] 18|TOP_ROOTS_SCAN:C:\\Users");
        assert!(crate::scanner::parse_scanner_progress(&line).is_some());
    }
}
