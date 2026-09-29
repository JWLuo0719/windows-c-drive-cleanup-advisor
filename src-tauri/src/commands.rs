use crate::cleanup::{self, ActionLogEntry, CleanupOutcome, CleanupPlan};
use crate::errors::AppError;
use crate::kernel::{self, KernelError, KernelOptions, SCAN_KERNEL_ENV, SCAN_KERNEL_POWERSHELL};
use crate::report::{
    build_scan_report, enriched_report_path_by_id, latest_enriched_report_path,
    latest_file_with_extension, list_report_summaries_from_root, parse_report_scan_id,
    parse_scanner_json, prune_report_dirs, read_enriched_report, register_completed_report,
    resolve_report_dir, resolve_reports_root, update_latest_report_index, RevealMode, ReportSummary,
    ScanReport, MAX_REPORT_HISTORY,
};
use crate::scanner::{
    for_each_stdout_line, normalize_drive, parse_scanner_progress, resolve_powershell,
    resolve_scanner_script, scanner_script_args, wait_child_with_watchdog, SCAN_STALL_TIMEOUT,
    WATCHDOG_POLL_INTERVAL,
};
use crate::tasks::{
    cancel_scan_task, complete_scan_task, current_phase, insert_queued_scan_task,
    try_register_process, update_task_status, ScanProgressEvent, ScanStatus, TaskMap,
};
use chrono::Utc;
use serde::Deserialize;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::Instant,
};
use tauri::{AppHandle, Emitter, State};
use uuid::Uuid;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) tasks: TaskMap,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScanOptions {
    drive: String,
    top_count: Option<u32>,
    large_file_mb: Option<u32>,
    scan_mode: Option<String>,
}

#[tauri::command]
pub(crate) fn start_scan(
    options: ScanOptions,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    let drive = normalize_drive(&options.drive)?;
    let scan_id = Uuid::new_v4().to_string();
    let started_at = Utc::now().to_rfc3339();
    let status = ScanStatus {
        scan_id: scan_id.clone(),
        phase: "queued".to_string(),
        percent: 2,
        message: "扫描已排队。本次不会执行任何清理动作。".to_string(),
        started_at: Some(started_at),
        completed_at: None,
        error: None,
        markdown_report_path: None,
        json_report_path: None,
    };

    insert_queued_scan_task(&state.tasks, &scan_id, status.clone())?;
    emit_progress(&app, &status);

    let tasks = state.tasks.clone();
    let top_count = options.top_count.unwrap_or(30).clamp(5, 100);
    let large_file_mb = options.large_file_mb.unwrap_or(200).clamp(50, 4096);
    let include_common_roots = options.scan_mode.as_deref() == Some("deep");
    let app_for_thread = app.clone();
    let scan_id_for_thread = scan_id.clone();

    thread::spawn(move || {
        if let Err(err) = run_scan_worker(
            app_for_thread.clone(),
            tasks.clone(),
            scan_id_for_thread.clone(),
            drive,
            top_count,
            large_file_mb,
            include_common_roots,
        ) {
            let status = update_task_status(
                &tasks,
                &scan_id_for_thread,
                "failed",
                100,
                "扫描失败。",
                Some(err.to_string()),
                None,
                None,
            );
            if let Some(next_status) = status {
                emit_progress(&app_for_thread, &next_status);
            }
        }
    });

    Ok(scan_id)
}

#[tauri::command]
pub(crate) fn get_scan_status(
    scan_id: String,
    state: State<'_, AppState>,
) -> Result<ScanStatus, AppError> {
    let tasks = state
        .tasks
        .lock()
        .map_err(|_| AppError::Message("扫描任务状态暂不可用。".to_string()))?;
    let task = tasks
        .get(&scan_id)
        .ok_or_else(|| AppError::Message(format!("未知扫描 ID: {scan_id}")))?;
    Ok(task.status.clone())
}

#[tauri::command]
pub(crate) fn cancel_scan(
    scan_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ScanStatus, AppError> {
    let (process_id, status, should_emit) = cancel_scan_task(&state.tasks, &scan_id)?;

    if let Some(pid) = process_id {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    if should_emit {
        emit_progress(&app, &status);
    }
    Ok(status)
}

#[tauri::command]
pub(crate) fn get_scan_report(
    scan_id: String,
    state: State<'_, AppState>,
) -> Result<ScanReport, AppError> {
    let tasks = state
        .tasks
        .lock()
        .map_err(|_| AppError::Message("扫描任务状态暂不可用。".to_string()))?;
    let task = tasks
        .get(&scan_id)
        .ok_or_else(|| AppError::Message(format!("未知扫描 ID: {scan_id}")))?;
    task.report
        .clone()
        .ok_or_else(|| AppError::Message("报告尚未生成完成。".to_string()))
}

#[tauri::command]
pub(crate) fn load_latest_report(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<ScanReport>, AppError> {
    let Some(report_path) = latest_enriched_report_path(&app)? else {
        return Ok(None);
    };
    let report = read_enriched_report(&report_path)?;
    register_completed_report(&state.tasks, report.clone())?;
    Ok(Some(report))
}

/// 历史报告摘要列表（Phase 3 发现层：历史与对比入口）。
#[tauri::command]
pub(crate) fn list_reports(app: AppHandle) -> Result<Vec<ReportSummary>, AppError> {
    let reports_root = resolve_reports_root(&app)?;
    list_report_summaries_from_root(&reports_root)
}

/// 按报告 ID 载入历史报告；ID 必须是纯 UUID，拒绝路径穿越类输入。
#[tauri::command]
pub(crate) fn load_report_by_id(
    scan_id: String,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ScanReport, AppError> {
    let scan_id = parse_report_scan_id(&scan_id)?;
    let reports_root = resolve_reports_root(&app)?;
    let report_path = enriched_report_path_by_id(&reports_root, &scan_id)
        .ok_or_else(|| AppError::Message("报告文件不存在，可能已被移动或删除。".to_string()))?;
    let report = read_enriched_report(&report_path)?;
    register_completed_report(&state.tasks, report.clone())?;
    Ok(report)
}

#[tauri::command]
pub(crate) fn reveal_report(
    scan_id: String,
    kind: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    let report = {
        let tasks = state
            .tasks
            .lock()
            .map_err(|_| AppError::Message("扫描任务状态暂不可用。".to_string()))?;
        let task = tasks
            .get(&scan_id)
            .ok_or_else(|| AppError::Message(format!("未知扫描 ID: {scan_id}")))?;
        task.report
            .clone()
            .ok_or_else(|| AppError::Message("报告尚未生成完成。".to_string()))?
    };

    let (target, mode) = resolve_reveal_target(&report, &kind)?;

    if !target.exists() {
        return Err(AppError::Message(
            "报告文件不存在，可能已被移动或删除。".to_string(),
        ));
    }

    let mut command = Command::new("explorer.exe");
    match mode {
        RevealMode::OpenFolder => {
            command.arg(&target);
        }
        RevealMode::SelectFile => {
            command.arg(format!("/select,{}", target.display()));
        }
    }
    command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

/// 按 reportId 反查已存储报告：cleanup 命令的唯一路径来源（UI 不得直传路径）。
fn load_report_for_cleanup(app: &AppHandle, report_id: &str) -> Result<ScanReport, AppError> {
    let scan_id = parse_report_scan_id(report_id)?;
    let reports_root = resolve_reports_root(app)?;
    let report_path = enriched_report_path_by_id(&reports_root, &scan_id)
        .ok_or_else(|| AppError::Message("报告文件不存在，可能已被移动或删除。".to_string()))?;
    read_enriched_report(&report_path)
}

/// Phase 5：dry-run 计划（不执行任何删除，只产出接受/拒绝明细）。
#[tauri::command]
pub(crate) fn plan_cleanup(
    report_id: String,
    candidate_ids: Vec<String>,
    app: AppHandle,
) -> Result<CleanupPlan, AppError> {
    let report = load_report_for_cleanup(&app, &report_id)?;
    cleanup::build_plan(&report, &report_id, &candidate_ids)
}

/// Phase 5：执行清理（与 plan 共用验证函数；逐项删除时刻复判；回收站优先；写 action-log）。
#[tauri::command]
pub(crate) fn execute_cleanup(
    report_id: String,
    candidate_ids: Vec<String>,
    app: AppHandle,
) -> Result<CleanupOutcome, AppError> {
    let report = load_report_for_cleanup(&app, &report_id)?;
    let reports_root = resolve_reports_root(&app)?;
    cleanup::execute(&report, &report_id, &candidate_ids, &reports_root)
}

/// Phase 5：action-log 审计读取（最新在前，上限见 cleanup::action_log_read_limit）。
#[tauri::command]
pub(crate) fn get_action_log(app: AppHandle) -> Result<Vec<ActionLogEntry>, AppError> {
    let reports_root = resolve_reports_root(&app)?;
    Ok(cleanup::read_action_log(
        &reports_root,
        cleanup::action_log_read_limit(),
    ))
}

fn resolve_reveal_target(
    report: &ScanReport,
    kind: &str,
) -> Result<(PathBuf, RevealMode), AppError> {
    match kind {
        "markdown" => Ok((
            PathBuf::from(&report.markdown_report_path),
            RevealMode::SelectFile,
        )),
        "json" => Ok((
            PathBuf::from(&report.json_report_path),
            RevealMode::SelectFile,
        )),
        "folder" => {
            let folder = PathBuf::from(&report.markdown_report_path)
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| AppError::Message("报告目录不可用。".to_string()))?;
            Ok((folder, RevealMode::OpenFolder))
        }
        _ => Err(AppError::Message("未知报告类型。".to_string())),
    }
}

/// 选择扫描内核：缺省原生（Phase 2 单层替换）；`WCDCA_SCAN_KERNEL=powershell` 时
/// 走 PS 兜底内核（等价性对照与资源缺失时的回退路径）。
fn select_scan_kernel() -> String {
    std::env::var(SCAN_KERNEL_ENV)
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default()
}

fn run_scan_worker(
    app: AppHandle,
    tasks: TaskMap,
    scan_id: String,
    drive: String,
    top_count: u32,
    large_file_mb: u32,
    include_common_roots: bool,
) -> Result<(), AppError> {
    let output_dir = resolve_report_dir(&app, &scan_id)?;
    fs::create_dir_all(&output_dir)?;
    if let Some(reports_root) = output_dir.parent() {
        prune_report_dirs(reports_root, MAX_REPORT_HISTORY, &scan_id);
    }

    let use_powershell = select_scan_kernel() == SCAN_KERNEL_POWERSHELL;
    let startup_message = if use_powershell {
        "正在启动内置只读 PowerShell 兜底扫描器。"
    } else {
        "正在启动内置只读扫描内核。"
    };
    let running = update_task_status(
        &tasks,
        &scan_id,
        "running",
        8,
        startup_message,
        None,
        None,
        None,
    );
    if running.is_none() {
        // 任务已在排队窗口被取消（或已不存在）：不得再启动扫描器，直接收尾。
        return Ok(());
    }
    if let Some(status) = running {
        emit_progress(&app, &status);
    }

    let (raw_json_path, markdown_path) = if use_powershell {
        run_powershell_scan(
            &app,
            &tasks,
            &scan_id,
            &drive,
            top_count,
            large_file_mb,
            include_common_roots,
            &output_dir,
        )?
    } else {
        run_native_scan(
            &app,
            &tasks,
            &scan_id,
            &drive,
            top_count,
            large_file_mb,
            include_common_roots,
            &output_dir,
        )?
    };

    let Some((raw_json_path, markdown_path)) = raw_json_path.zip(markdown_path) else {
        // 扫描被取消（或未产出报告）：保持终态不变，直接收尾。
        return Ok(());
    };

    if let Some(status) = update_task_status(
        &tasks,
        &scan_id,
        "running",
        76,
        "扫描完成。正在分类结果并写入增强报告。",
        None,
        None,
        None,
    ) {
        emit_progress(&app, &status);
    }

    let raw_json = fs::read_to_string(&raw_json_path)?;
    let raw = parse_scanner_json(&raw_json)?;
    let enriched_json_path = output_dir.join(format!("scan-report-{scan_id}.json"));
    let report = build_scan_report(&scan_id, &drive, &raw, &markdown_path, &enriched_json_path)?;
    fs::write(&enriched_json_path, serde_json::to_string_pretty(&report)?)?;
    if let Some(reports_root) = output_dir.parent() {
        update_latest_report_index(reports_root, &scan_id);
    }

    let completed = complete_scan_task(&tasks, &scan_id, report)?;

    if let Some(status) = completed {
        emit_progress(&app, &status);
    }
    Ok(())
}

/// 原生内核扫描：进程内遍历，进度标记经 parse_scanner_progress 复用协议消息模板。
/// 返回 (json, md)；取消时返回 (None, None)。
fn run_native_scan(
    app: &AppHandle,
    tasks: &TaskMap,
    scan_id: &str,
    drive: &str,
    top_count: u32,
    large_file_mb: u32,
    include_common_roots: bool,
    output_dir: &Path,
) -> Result<(Option<PathBuf>, Option<PathBuf>), AppError> {
    let drive_root = format!("{drive}:\\");
    let options = KernelOptions::new(&drive_root, top_count as usize, large_file_mb as u64, include_common_roots);

    let progress = |percent: u8, code_with_path: &str| -> bool {
        if current_phase(tasks, scan_id).as_deref() == Some("cancelled") {
            return false;
        }
        let line = kernel::format_marker_line(percent, code_with_path);
        if let Some((percent, message)) = parse_scanner_progress(&line) {
            if let Some(status) =
                update_task_status(tasks, scan_id, "running", percent, &message, None, None, None)
            {
                emit_progress(app, &status);
            }
        }
        true
    };

    match kernel::run_scan(&options, &progress) {
        Ok(result) => {
            let (markdown_path, json_path) = kernel::write_raw_reports(
                &result,
                output_dir,
                &|percent: u8, code_with_path: &str| {
                    let line = kernel::format_marker_line(percent, code_with_path);
                    if let Some((percent, message)) = parse_scanner_progress(&line) {
                        if let Some(status) = update_task_status(
                            tasks, scan_id, "running", percent, &message, None, None, None,
                        ) {
                            emit_progress(app, &status);
                        }
                    }
                    true
                },
            )
            .map_err(|err| AppError::Message(err.to_string()))?;
            Ok((Some(json_path), Some(markdown_path)))
        }
        Err(KernelError::Cancelled) => Ok((None, None)),
        Err(KernelError::Failed(message)) => Err(AppError::Message(message)),
    }
}

/// PS 兜底内核扫描：以固定参数启动捆绑脚本（安全边界断言看护该路径）。
/// 返回 (json, md)；取消时返回 (None, None)。
fn run_powershell_scan(
    app: &AppHandle,
    tasks: &TaskMap,
    scan_id: &str,
    drive: &str,
    top_count: u32,
    large_file_mb: u32,
    include_common_roots: bool,
    output_dir: &Path,
) -> Result<(Option<PathBuf>, Option<PathBuf>), AppError> {
    let script_path = resolve_scanner_script(app)?;

    let shell = resolve_powershell();
    let mut command = Command::new(&shell);
    command
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File");

    for arg in scanner_script_args(
        &script_path,
        drive,
        output_dir,
        top_count,
        large_file_mb,
        include_common_roots,
    ) {
        command.arg(arg);
    }

    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    // PID 登记与取消在同一把锁下互斥：
    // 取消先到则登记被拒，由本线程自行收尾刚启动的进程树；登记先到则取消方拿走 PID 去 taskkill。
    if !try_register_process(tasks, scan_id, child.id()) {
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = child.wait();
        return Ok((None, None));
    }

    let last_activity = Arc::new(Mutex::new(Instant::now()));

    let stdout_handle = child.stdout.take().map(|stdout| {
        let app = app.clone();
        let tasks = tasks.clone();
        let scan_id = scan_id.to_string();
        let last_activity = last_activity.clone();
        thread::spawn(move || {
            // 必须按字节逐行读取并容忍任意编码：PowerShell 5.1 管道输出走 OEM 码页
            // （简体中文系统为 GBK），含中文路径的进度行不是合法 UTF-8；
            // 若按严格 UTF-8 读行，首个中文行就会终止本线程，
            // 进度与看门狗活性信号（last_activity）同时永久冻结。
            for_each_stdout_line(stdout, |line| {
                if let Ok(mut stamp) = last_activity.lock() {
                    *stamp = Instant::now();
                }
                if let Some((percent, message)) = parse_scanner_progress(line) {
                    if current_phase(&tasks, &scan_id).as_deref() == Some("cancelled") {
                        return false;
                    }
                    if let Some(status) = update_task_status(
                        &tasks, &scan_id, "running", percent, &message, None, None, None,
                    ) {
                        emit_progress(&app, &status);
                    }
                }
                true
            })
        })
    });

    let stderr_handle = child.stderr.take().map(|mut stderr| {
        thread::spawn(move || {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            text
        })
    });

    let (output_status, stalled) = wait_child_with_watchdog(
        &mut child,
        &last_activity,
        SCAN_STALL_TIMEOUT,
        WATCHDOG_POLL_INTERVAL,
    )?;
    if let Some(handle) = stdout_handle {
        let _ = handle.join();
    }
    let stderr = stderr_handle
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();

    let was_cancelled = current_phase(tasks, scan_id).as_deref() == Some("cancelled");
    if was_cancelled {
        return Ok((None, None));
    }

    if stalled {
        return Err(AppError::Message(format!(
            "扫描器 {} 分钟内没有任何进度输出，已判定卡死并强制终止。请重新扫描。",
            SCAN_STALL_TIMEOUT.as_secs() / 60
        )));
    }

    if !output_status.success() {
        let detail = stderr.trim().to_string();
        return Err(AppError::Message(format!(
            "PowerShell 扫描器退出状态为 {}。{detail}",
            output_status
        )));
    }

    let raw_json_path = latest_file_with_extension(output_dir, "json")
        .ok_or_else(|| AppError::Message("扫描内核没有生成 JSON 输出。".to_string()))?;
    let markdown_path = latest_file_with_extension(output_dir, "md")
        .ok_or_else(|| AppError::Message("扫描内核没有生成 Markdown 输出。".to_string()))?;
    Ok((Some(raw_json_path), Some(markdown_path)))
}

fn emit_progress(app: &AppHandle, status: &ScanStatus) {
    let _ = app.emit(
        "scan-progress",
        ScanProgressEvent {
            scan_id: status.scan_id.clone(),
            phase: status.phase.clone(),
            percent: status.percent,
            message: status.message.clone(),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::test_scan_report;

    #[test]
    fn reveal_target_resolves_report_files_and_folder() {
        let report = test_scan_report("C:\\reports\\scan.md", "C:\\reports\\scan.json");

        let (markdown_target, markdown_mode) = resolve_reveal_target(&report, "markdown").unwrap();
        assert_eq!(markdown_target, PathBuf::from("C:\\reports\\scan.md"));
        assert_eq!(markdown_mode, RevealMode::SelectFile);

        let (json_target, json_mode) = resolve_reveal_target(&report, "json").unwrap();
        assert_eq!(json_target, PathBuf::from("C:\\reports\\scan.json"));
        assert_eq!(json_mode, RevealMode::SelectFile);

        let (folder_target, folder_mode) = resolve_reveal_target(&report, "folder").unwrap();
        assert_eq!(folder_target, PathBuf::from("C:\\reports"));
        assert_eq!(folder_mode, RevealMode::OpenFolder);
    }

    #[test]
    fn reveal_target_rejects_unknown_report_kind() {
        let report = test_scan_report("C:\\reports\\scan.md", "C:\\reports\\scan.json");
        let error = resolve_reveal_target(&report, "exe")
            .unwrap_err()
            .to_string();

        assert!(error.contains("未知报告类型"));
    }
}
