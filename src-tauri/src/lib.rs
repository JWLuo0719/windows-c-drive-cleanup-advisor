use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
};
use tauri::{AppHandle, Emitter, Manager, State};
use thiserror::Error;
use uuid::Uuid;

type TaskMap = Arc<Mutex<HashMap<String, ScanTask>>>;

#[derive(Clone)]
struct AppState {
    tasks: TaskMap,
}

#[derive(Debug, Error)]
enum AppError {
    #[error("{0}")]
    Message(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScanOptions {
    drive: String,
    top_count: Option<u32>,
    large_file_mb: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanStatus {
    scan_id: String,
    phase: String,
    percent: u8,
    message: String,
    started_at: Option<String>,
    completed_at: Option<String>,
    error: Option<String>,
    markdown_report_path: Option<String>,
    json_report_path: Option<String>,
}

#[derive(Debug, Clone)]
struct ScanTask {
    status: ScanStatus,
    report: Option<ScanReport>,
    process_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanProgressEvent {
    scan_id: String,
    phase: String,
    percent: u8,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScanReport {
    schema_version: String,
    scan_id: String,
    created_at: String,
    drive: String,
    is_elevated: bool,
    skipped_reparse_points: u64,
    scan_errors: Vec<String>,
    privacy: PrivacyLedger,
    recommendations: Vec<Recommendation>,
    markdown_report_path: String,
    json_report_path: String,
}

#[derive(Debug, Clone, Serialize)]
struct PrivacyLedger {
    uploaded: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Recommendation {
    id: String,
    path: String,
    size_gb: f64,
    category: String,
    risk: String,
    confidence: f64,
    reason: String,
    manual_steps: Vec<String>,
    cleanable: bool,
    blocked_reason: Option<String>,
    cleanup_method: Option<String>,
    requires_app_closed: bool,
    source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RevealMode {
    SelectFile,
    OpenFolder,
}

#[tauri::command]
fn start_scan(
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

    {
        let mut tasks = state
            .tasks
            .lock()
            .map_err(|_| AppError::Message("扫描任务状态暂不可用。".to_string()))?;
        tasks.insert(
            scan_id.clone(),
            ScanTask {
                status: status.clone(),
                report: None,
                process_id: None,
            },
        );
    }
    emit_progress(&app, &status);

    let tasks = state.tasks.clone();
    let top_count = options.top_count.unwrap_or(30).clamp(5, 100);
    let large_file_mb = options.large_file_mb.unwrap_or(200).clamp(50, 4096);
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
fn get_scan_status(scan_id: String, state: State<'_, AppState>) -> Result<ScanStatus, AppError> {
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
fn cancel_scan(
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
fn get_scan_report(scan_id: String, state: State<'_, AppState>) -> Result<ScanReport, AppError> {
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
fn reveal_report(
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

fn run_scan_worker(
    app: AppHandle,
    tasks: TaskMap,
    scan_id: String,
    drive: String,
    top_count: u32,
    large_file_mb: u32,
) -> Result<(), AppError> {
    let script_path = resolve_scanner_script(&app)?;
    let output_dir = resolve_report_dir(&app, &scan_id)?;
    fs::create_dir_all(&output_dir)?;

    let running = update_task_status(
        &tasks,
        &scan_id,
        "running",
        8,
        "正在启动内置只读 PowerShell 扫描器。",
        None,
        None,
        None,
    );
    if let Some(status) = running {
        emit_progress(&app, &status);
    }

    let shell = resolve_powershell();
    let mut child = Command::new(&shell)
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(&script_path)
        .arg("-Drive")
        .arg(&drive)
        .arg("-OutputDir")
        .arg(&output_dir)
        .arg("-TopCount")
        .arg(top_count.to_string())
        .arg("-LargeFileMB")
        .arg(large_file_mb.to_string())
        .arg("-IncludeJson")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    {
        let mut locked = tasks
            .lock()
            .map_err(|_| AppError::Message("扫描任务状态暂不可用。".to_string()))?;
        if let Some(task) = locked.get_mut(&scan_id) {
            task.process_id = Some(child.id());
        }
    }

    let stdout_handle = child.stdout.take().map(|stdout| {
        let app = app.clone();
        let tasks = tasks.clone();
        let scan_id = scan_id.clone();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if let Some((percent, message)) = parse_scanner_progress(&line) {
                    if current_phase(&tasks, &scan_id).as_deref() == Some("cancelled") {
                        break;
                    }
                    if let Some(status) = update_task_status(
                        &tasks, &scan_id, "running", percent, &message, None, None, None,
                    ) {
                        emit_progress(&app, &status);
                    }
                }
            }
        })
    });

    let stderr_handle = child.stderr.take().map(|mut stderr| {
        thread::spawn(move || {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            text
        })
    });

    let output_status = child.wait()?;
    if let Some(handle) = stdout_handle {
        let _ = handle.join();
    }
    let stderr = stderr_handle
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();

    let was_cancelled = current_phase(&tasks, &scan_id).as_deref() == Some("cancelled");
    if was_cancelled {
        return Ok(());
    }

    if !output_status.success() {
        let detail = stderr.trim().to_string();
        return Err(AppError::Message(format!(
            "PowerShell 扫描器退出状态为 {}。{detail}",
            output_status
        )));
    }

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

    let raw_json_path = latest_file_with_extension(&output_dir, "json")
        .ok_or_else(|| AppError::Message("PowerShell 扫描器没有生成 JSON 输出。".to_string()))?;
    let markdown_path = latest_file_with_extension(&output_dir, "md").ok_or_else(|| {
        AppError::Message("PowerShell 扫描器没有生成 Markdown 输出。".to_string())
    })?;
    let raw_json = fs::read_to_string(&raw_json_path)?;
    let raw: Value = serde_json::from_str(&raw_json)?;
    let enriched_json_path = output_dir.join(format!("scan-report-{scan_id}.json"));
    let report = build_scan_report(&scan_id, &drive, &raw, &markdown_path, &enriched_json_path)?;
    fs::write(&enriched_json_path, serde_json::to_string_pretty(&report)?)?;

    let completed = complete_scan_task(&tasks, &scan_id, report)?;

    if let Some(status) = completed {
        emit_progress(&app, &status);
    }
    Ok(())
}

fn normalize_drive(input: &str) -> Result<String, AppError> {
    let trimmed = input.trim().trim_end_matches(':');
    let mut chars = trimmed.chars();
    let letter = chars
        .next()
        .ok_or_else(|| AppError::Message("必须指定盘符。".to_string()))?;
    if chars.next().is_some() || !letter.is_ascii_alphabetic() {
        return Err(AppError::Message(
            "盘符必须是单个 Windows 驱动器字母。".to_string(),
        ));
    }
    Ok(letter.to_ascii_uppercase().to_string())
}

fn resolve_scanner_script(app: &AppHandle) -> Result<PathBuf, AppError> {
    #[cfg(debug_assertions)]
    {
        if let Some(dev_path) = dev_scanner_script_path() {
            return Ok(dev_path);
        }
    }

    let resource_dir = app
        .path()
        .resource_dir()
        .map_err(|err| AppError::Message(err.to_string()))?;
    resource_scanner_candidates(&resource_dir)
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| AppError::Message("没有找到内置扫描脚本。".to_string()))
}

#[cfg(debug_assertions)]
fn dev_scanner_script_path() -> Option<PathBuf> {
    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("scripts")
        .join("Scan-CDriveCleanupAdvisor.ps1");
    if dev_path.exists() {
        Some(dev_path)
    } else {
        None
    }
}

fn resource_scanner_candidates(resource_dir: &Path) -> Vec<PathBuf> {
    vec![
        resource_dir
            .join("scripts")
            .join("Scan-CDriveCleanupAdvisor.ps1"),
        resource_dir.join("Scan-CDriveCleanupAdvisor.ps1"),
    ]
}

fn resolve_report_dir(app: &AppHandle, scan_id: &str) -> Result<PathBuf, AppError> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|err| AppError::Message(err.to_string()))?;
    Ok(base.join("reports").join(scan_id))
}

fn resolve_powershell() -> String {
    if Command::new("pwsh")
        .arg("-NoLogo")
        .arg("-NoProfile")
        .arg("-Command")
        .arg("$PSVersionTable.PSVersion.ToString()")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
    {
        "pwsh".to_string()
    } else {
        "powershell.exe".to_string()
    }
}

fn latest_file_with_extension(dir: &Path, extension: &str) -> Option<PathBuf> {
    let mut files = fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some(extension))
        .filter_map(|path| {
            let modified = fs::metadata(&path).and_then(|meta| meta.modified()).ok()?;
            Some((modified, path))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|(modified, _)| *modified);
    files.pop().map(|(_, path)| path)
}

fn build_scan_report(
    scan_id: &str,
    drive: &str,
    raw: &Value,
    markdown_path: &Path,
    enriched_json_path: &Path,
) -> Result<ScanReport, AppError> {
    let mut recommendations = Vec::new();
    let mut seen = HashSet::new();

    collect_rows(raw.get("top"), &mut recommendations, &mut seen, "scanner");
    if let Some(drilldowns) = raw.get("drilldowns").and_then(Value::as_object) {
        for rows in drilldowns.values() {
            collect_rows(Some(rows), &mut recommendations, &mut seen, "scanner");
        }
    }
    collect_rows(
        raw.get("largeFiles"),
        &mut recommendations,
        &mut seen,
        "heuristic",
    );

    recommendations.sort_by(|a, b| {
        b.size_gb
            .partial_cmp(&a.size_gb)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    recommendations.truncate(80);

    Ok(ScanReport {
        schema_version: "0.1.0".to_string(),
        scan_id: scan_id.to_string(),
        created_at: Utc::now().to_rfc3339(),
        drive: format!("{drive}:"),
        is_elevated: raw.get("isAdmin").and_then(Value::as_bool).unwrap_or(false),
        skipped_reparse_points: count_skipped_reparse_points(raw),
        scan_errors: collect_scan_errors(raw),
        privacy: PrivacyLedger { uploaded: false },
        recommendations,
        markdown_report_path: markdown_path.display().to_string(),
        json_report_path: enriched_json_path.display().to_string(),
    })
}

fn collect_rows(
    source: Option<&Value>,
    recommendations: &mut Vec<Recommendation>,
    seen: &mut HashSet<String>,
    source_name: &str,
) {
    let Some(rows) = source.and_then(Value::as_array) else {
        return;
    };

    for row in rows {
        let Some(path) = row
            .get("Path")
            .or_else(|| row.get("FullName"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        let size_gb = row
            .get("SizeGB")
            .or_else(|| row.get("sizeGb"))
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        if size_gb <= 0.0 {
            continue;
        }
        let key = path.to_ascii_lowercase();
        if !seen.insert(key) {
            continue;
        }
        recommendations.push(classify_recommendation(path, size_gb, source_name));
    }
}

fn classify_recommendation(path: &str, size_gb: f64, source_name: &str) -> Recommendation {
    let lower = path.to_ascii_lowercase();
    let id = Uuid::new_v4().to_string();

    if is_system_managed(&lower) {
        return Recommendation {
            id,
            path: path.to_string(),
            size_gb,
            category: "system-managed".to_string(),
            risk: "blocked".to_string(),
            confidence: 0.96,
            reason: "这是由 Windows 管理的位置。手动删除可能影响系统修复、更新、启动或回滚能力。"
                .to_string(),
            manual_steps: vec![
                "仅使用 Windows 设置、磁盘清理、DISM 或官方说明中的系统工具处理。".to_string(),
                "不要直接删除这个路径。".to_string(),
            ],
            cleanable: false,
            blocked_reason: Some("系统托管路径，不提供自动清理。".to_string()),
            cleanup_method: Some("manual-windows-tool".to_string()),
            requires_app_closed: false,
            source: source_name.to_string(),
        };
    }

    if is_low_risk_cache(&lower) {
        return Recommendation {
            id,
            path: path.to_string(),
            size_gb,
            category: "low-risk-cache".to_string(),
            risk: "low".to_string(),
            confidence: 0.82,
            reason: "这看起来像缓存或更新包目录，相关应用通常会在后续使用中重新生成。".to_string(),
            manual_steps: vec![
                "先关闭相关应用。".to_string(),
                "优先使用应用内清理入口；如需手动处理，请先确认内容。".to_string(),
            ],
            cleanable: false,
            blocked_reason: Some("内置清理已后移到 v0.3 白名单流程。".to_string()),
            cleanup_method: Some("manual-cache-review".to_string()),
            requires_app_closed: true,
            source: source_name.to_string(),
        };
    }

    if is_app_managed(&lower) {
        return Recommendation {
            id,
            path: path.to_string(),
            size_gb,
            category: "app-managed".to_string(),
            risk: "medium".to_string(),
            confidence: 0.85,
            reason: "这看起来是应用托管数据。聊天软件、网盘和编辑器可能在这里保存数据库、索引或本地副本。".to_string(),
            manual_steps: vec![
                "优先使用应用自带的存储管理功能。".to_string(),
                "手动删除前请先备份或逐项确认。".to_string(),
            ],
            cleanable: false,
            blocked_reason: Some("应用托管数据需要用户人工确认。".to_string()),
            cleanup_method: Some("app-storage-manager".to_string()),
            requires_app_closed: true,
            source: source_name.to_string(),
        };
    }

    if is_uninstall_or_migrate(&lower) {
        return Recommendation {
            id,
            path: path.to_string(),
            size_gb,
            category: "uninstall-or-migrate".to_string(),
            risk: "high".to_string(),
            confidence: 0.78,
            reason: "这看起来像已安装软件、SDK、组件包或大型应用目录。".to_string(),
            manual_steps: vec![
                "请使用 Windows 应用设置或厂商卸载器处理。".to_string(),
                "迁移项目、SDK 或工具缓存前，先确认依赖它们的工具不会受影响。".to_string(),
            ],
            cleanable: false,
            blocked_reason: Some("已安装软件不应直接删除。".to_string()),
            cleanup_method: Some("uninstall-or-migrate".to_string()),
            requires_app_closed: true,
            source: source_name.to_string(),
        };
    }

    Recommendation {
        id,
        path: path.to_string(),
        size_gb,
        category: "user-data".to_string(),
        risk: "medium".to_string(),
        confidence: 0.7,
        reason: "这是用户可见或类似项目的数据，可能包含下载、文档、媒体、源码或导出文件。"
            .to_string(),
        manual_steps: vec![
            "打开文件夹并按大小排序。".to_string(),
            "确认不再需要后，再移动、归档或删除。".to_string(),
        ],
        cleanable: false,
        blocked_reason: Some("用户数据需要明确的人工确认。".to_string()),
        cleanup_method: Some("manual-review".to_string()),
        requires_app_closed: false,
        source: source_name.to_string(),
    }
}

fn is_system_managed(lower: &str) -> bool {
    lower == "c:\\pagefile.sys"
        || lower == "c:\\swapfile.sys"
        || lower == "c:\\hiberfil.sys"
        || lower.contains("\\windows\\winsxs")
        || lower.contains("\\windows\\installer")
        || lower.contains("\\windows\\system32")
        || lower.contains("\\windows\\servicing")
        || lower.contains("\\system volume information")
}

fn is_low_risk_cache(lower: &str) -> bool {
    lower.contains("\\cache")
        || lower.contains("\\code cache")
        || lower.contains("\\gpucache")
        || lower.contains("\\dxcache")
        || lower.contains("\\glcache")
        || lower.contains("\\npm-cache")
        || lower.contains("\\pip\\cache")
        || lower.contains("\\.gradle\\caches")
        || lower.contains("\\ms-playwright")
        || lower.contains("\\cachedextensionvsixs")
        || lower.contains("\\autoupdate\\download")
        || lower.contains("\\update")
}

fn is_app_managed(lower: &str) -> bool {
    lower.contains("\\tencent")
        || lower.contains("\\wechat")
        || lower.contains("\\wxwork")
        || lower.contains("\\qq")
        || lower.contains("\\kingsoft")
        || lower.contains("\\wps")
        || lower.contains("\\onedrive")
}

fn is_uninstall_or_migrate(lower: &str) -> bool {
    lower.contains("\\program files")
        || lower.contains("\\program files (x86)")
        || lower.contains("\\programdata")
        || lower.contains("\\appdata\\local\\programs")
        || lower.contains("\\appdata\\local\\packages")
        || lower.ends_with("\\ext4.vhdx")
}

fn count_skipped_reparse_points(raw: &Value) -> u64 {
    let mut total = 0;
    add_skipped(raw.get("top"), &mut total);
    if let Some(drilldowns) = raw.get("drilldowns").and_then(Value::as_object) {
        for rows in drilldowns.values() {
            add_skipped(Some(rows), &mut total);
        }
    }
    total
}

fn collect_scan_errors(raw: &Value) -> Vec<String> {
    let Some(errors) = raw.get("scanErrors").and_then(Value::as_array) else {
        return Vec::new();
    };

    errors
        .iter()
        .take(200)
        .filter_map(|item| {
            if let Some(text) = item.as_str() {
                let trimmed = text.trim();
                return (!trimmed.is_empty()).then(|| trimmed.to_string());
            }

            let stage = item
                .get("Stage")
                .or_else(|| item.get("stage"))
                .and_then(Value::as_str)
                .unwrap_or("Scan");
            let path = item
                .get("Path")
                .or_else(|| item.get("path"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let message = item
                .get("Message")
                .or_else(|| item.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("Unreadable path");

            let combined = if path.is_empty() {
                format!("{stage}: {message}")
            } else {
                format!("{stage}: {path} - {message}")
            };
            Some(combined)
        })
        .collect()
}

fn add_skipped(source: Option<&Value>, total: &mut u64) {
    let Some(rows) = source.and_then(Value::as_array) else {
        return;
    };
    for row in rows {
        if let Some(count) = row.get("SkippedReparsePoints").and_then(Value::as_u64) {
            *total += count;
        }
    }
}

fn update_task_status(
    tasks: &TaskMap,
    scan_id: &str,
    phase: &str,
    percent: u8,
    message: &str,
    error: Option<String>,
    markdown_report_path: Option<String>,
    json_report_path: Option<String>,
) -> Option<ScanStatus> {
    let mut locked = tasks.lock().ok()?;
    let task = locked.get_mut(scan_id)?;
    let status = &mut task.status;
    status.phase = phase.to_string();
    status.percent = percent;
    status.message = message.to_string();
    status.error = error;
    if ["completed", "failed", "cancelled"].contains(&phase) {
        status.completed_at = Some(Utc::now().to_rfc3339());
        task.process_id = None;
    }
    if markdown_report_path.is_some() {
        status.markdown_report_path = markdown_report_path;
    }
    if json_report_path.is_some() {
        status.json_report_path = json_report_path;
    }
    Some(status.clone())
}

fn is_terminal_phase(phase: &str) -> bool {
    matches!(phase, "completed" | "failed" | "cancelled")
}

fn cancel_scan_task(
    tasks: &TaskMap,
    scan_id: &str,
) -> Result<(Option<u32>, ScanStatus, bool), AppError> {
    let mut locked = tasks
        .lock()
        .map_err(|_| AppError::Message("扫描任务状态暂不可用。".to_string()))?;
    let task = locked
        .get_mut(scan_id)
        .ok_or_else(|| AppError::Message(format!("未知扫描 ID: {scan_id}")))?;

    if is_terminal_phase(&task.status.phase) {
        task.process_id = None;
        return Ok((None, task.status.clone(), false));
    }

    let process_id = task.process_id.take();
    let status = &mut task.status;
    status.phase = "cancelled".to_string();
    status.percent = 100;
    status.message = "扫描已取消。本次未执行任何清理动作。".to_string();
    status.error = None;
    status.completed_at = Some(Utc::now().to_rfc3339());

    Ok((process_id, status.clone(), true))
}
fn complete_scan_task(
    tasks: &TaskMap,
    scan_id: &str,
    report: ScanReport,
) -> Result<Option<ScanStatus>, AppError> {
    let markdown_report_path = report.markdown_report_path.clone();
    let json_report_path = report.json_report_path.clone();
    let mut locked = tasks
        .lock()
        .map_err(|_| AppError::Message("扫描任务状态暂不可用。".to_string()))?;
    let Some(task) = locked.get_mut(scan_id) else {
        return Ok(None);
    };

    task.report = Some(report);
    task.process_id = None;

    let status = &mut task.status;
    status.phase = "completed".to_string();
    status.percent = 100;
    status.message = "扫描完成。本次没有删除、移动或修改任何文件。".to_string();
    status.error = None;
    status.completed_at = Some(Utc::now().to_rfc3339());
    status.markdown_report_path = Some(markdown_report_path);
    status.json_report_path = Some(json_report_path);

    Ok(Some(status.clone()))
}

fn current_phase(tasks: &TaskMap, scan_id: &str) -> Option<String> {
    let locked = tasks.lock().ok()?;
    locked.get(scan_id).map(|task| task.status.phase.clone())
}

fn parse_scanner_progress(line: &str) -> Option<(u8, String)> {
    let payload = line.strip_prefix("[WCDCA_PROGRESS] ")?;
    let (percent_text, code) = payload.split_once('|')?;
    let percent = percent_text.trim().parse::<u8>().ok()?.min(99);
    let message = match code.trim() {
        "DRIVE_INFO" => "正在读取磁盘容量和权限信息。".to_string(),
        "TOP_ROOTS" => "正在扫描 C 盘顶层真实目录。".to_string(),
        "LARGE_FILES" => "正在查找大文件。".to_string(),
        "SYSTEM_INFO" => "正在读取 pagefile、休眠和系统托管项信息。".to_string(),
        "DISM" => "正在读取 DISM 组件存储分析。".to_string(),
        "REPORT" => "正在生成 Markdown 报告。".to_string(),
        "JSON" => "正在写入 JSON 数据。".to_string(),
        other if other.starts_with("DRILLDOWN:") => {
            let path = other.trim_start_matches("DRILLDOWN:");
            if path.is_empty() {
                "正在扫描重点目录。".to_string()
            } else {
                format!("正在扫描重点目录：{path}")
            }
        }
        _ => "扫描正在进行。".to_string(),
    };
    Some((percent, message))
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

pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            tasks: Arc::new(Mutex::new(HashMap::new())),
        })
        .invoke_handler(tauri::generate_handler![
            start_scan,
            get_scan_status,
            cancel_scan,
            get_scan_report,
            reveal_report
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn test_scan_report(markdown_report_path: &str, json_report_path: &str) -> ScanReport {
        ScanReport {
            schema_version: "0.1.0".to_string(),
            scan_id: "scan-test".to_string(),
            created_at: "2026-06-21T00:00:01Z".to_string(),
            drive: "C:".to_string(),
            is_elevated: false,
            skipped_reparse_points: 0,
            scan_errors: Vec::new(),
            privacy: PrivacyLedger { uploaded: false },
            recommendations: Vec::new(),
            markdown_report_path: markdown_report_path.to_string(),
            json_report_path: json_report_path.to_string(),
        }
    }
    #[test]
    fn normalize_drive_accepts_single_letter_only() {
        assert_eq!(normalize_drive("c").unwrap(), "C");
        assert_eq!(normalize_drive("D:").unwrap(), "D");
        assert!(normalize_drive("CC").is_err());
        assert!(normalize_drive("").is_err());
    }

    #[test]
    fn system_managed_recommendation_is_blocked_and_not_cleanable() {
        let recommendation = classify_recommendation("C:\\Windows\\WinSxS", 18.4, "scanner");

        assert_eq!(recommendation.category, "system-managed");
        assert_eq!(recommendation.risk, "blocked");
        assert!(!recommendation.cleanable);
        assert_eq!(
            recommendation.blocked_reason.as_deref(),
            Some("系统托管路径，不提供自动清理。")
        );
    }

    #[test]
    fn cache_recommendation_stays_manual_until_allowlist_release() {
        let recommendation = classify_recommendation(
            "C:\\Users\\me\\AppData\\Local\\NVIDIA\\DXCache",
            2.1,
            "scanner",
        );

        assert_eq!(recommendation.category, "low-risk-cache");
        assert_eq!(recommendation.risk, "low");
        assert!(!recommendation.cleanable);
        assert_eq!(
            recommendation.blocked_reason.as_deref(),
            Some("内置清理已后移到 v0.3 白名单流程。")
        );
    }

    #[test]
    fn scan_report_counts_reparse_points_and_deduplicates_paths() {
        let raw = json!({
            "isAdmin": false,
            "top": [
                {
                    "Path": "C:\\Users\\me\\Downloads",
                    "SizeGB": 5.0,
                    "SkippedReparsePoints": 2
                }
            ],
            "drilldowns": {
                "C:\\Users\\me": [
                    {
                        "Path": "C:\\Users\\me\\Downloads",
                        "SizeGB": 5.0,
                        "SkippedReparsePoints": 3
                    },
                    {
                        "Path": "C:\\Users\\me\\AppData\\Local\\Temp\\Cache",
                        "SizeGB": 1.5,
                        "SkippedReparsePoints": 1
                    }
                ]
            },
            "largeFiles": [],
            "scanErrors": [
                {
                    "Stage": "TreeSizeEnumerate",
                    "Path": "C:\\System Volume Information",
                    "Message": "Access denied"
                }
            ]
        });

        let report = build_scan_report(
            "scan-1",
            "C",
            &raw,
            Path::new("report.md"),
            Path::new("report.json"),
        )
        .unwrap();

        assert_eq!(report.schema_version, "0.1.0");
        assert_eq!(report.drive, "C:");
        assert!(!report.is_elevated);
        assert_eq!(report.skipped_reparse_points, 6);
        assert_eq!(
            report.scan_errors,
            vec!["TreeSizeEnumerate: C:\\System Volume Information - Access denied"]
        );
        assert_eq!(report.recommendations.len(), 2);
        assert!(report
            .recommendations
            .iter()
            .any(|item| item.category == "low-risk-cache"));
    }

    #[test]
    fn scan_errors_support_string_items_and_limit_output() {
        let raw = json!({
            "scanErrors": ["first", "", "second"]
        });

        assert_eq!(collect_scan_errors(&raw), vec!["first", "second"]);

        let many = json!({
            "scanErrors": (0..205).map(|index| format!("error-{index}")).collect::<Vec<_>>()
        });
        assert_eq!(collect_scan_errors(&many).len(), 200);
    }

    #[test]
    fn cancel_scan_task_marks_running_task_cancelled_and_returns_pid() {
        let scan_id = "scan-cancel";
        let tasks: TaskMap = Arc::new(Mutex::new(HashMap::new()));
        let status = ScanStatus {
            scan_id: scan_id.to_string(),
            phase: "running".to_string(),
            percent: 44,
            message: "scanning".to_string(),
            started_at: Some("2026-06-21T00:00:00Z".to_string()),
            completed_at: None,
            error: None,
            markdown_report_path: None,
            json_report_path: None,
        };

        {
            let mut locked = tasks.lock().unwrap();
            locked.insert(
                scan_id.to_string(),
                ScanTask {
                    status,
                    report: None,
                    process_id: Some(456),
                },
            );
        }

        let (process_id, cancelled, should_emit) = cancel_scan_task(&tasks, scan_id).unwrap();

        assert_eq!(process_id, Some(456));
        assert!(should_emit);
        assert_eq!(cancelled.phase, "cancelled");
        assert_eq!(cancelled.percent, 100);
        assert!(cancelled.completed_at.is_some());

        let locked = tasks.lock().unwrap();
        let task = locked.get(scan_id).unwrap();
        assert!(task.process_id.is_none());
        assert_eq!(task.status.phase, "cancelled");
    }

    #[test]
    fn cancel_scan_task_does_not_overwrite_terminal_status() {
        let scan_id = "scan-completed";
        let tasks: TaskMap = Arc::new(Mutex::new(HashMap::new()));
        let status = ScanStatus {
            scan_id: scan_id.to_string(),
            phase: "completed".to_string(),
            percent: 100,
            message: "finished".to_string(),
            started_at: Some("2026-06-21T00:00:00Z".to_string()),
            completed_at: Some("2026-06-21T00:01:00Z".to_string()),
            error: None,
            markdown_report_path: Some("C:\\reports\\scan.md".to_string()),
            json_report_path: Some("C:\\reports\\scan.json".to_string()),
        };

        {
            let mut locked = tasks.lock().unwrap();
            locked.insert(
                scan_id.to_string(),
                ScanTask {
                    status,
                    report: None,
                    process_id: Some(789),
                },
            );
        }

        let (process_id, returned, should_emit) = cancel_scan_task(&tasks, scan_id).unwrap();

        assert_eq!(process_id, None);
        assert!(!should_emit);
        assert_eq!(returned.phase, "completed");
        assert_eq!(returned.message, "finished");
        assert_eq!(
            returned.markdown_report_path.as_deref(),
            Some("C:\\reports\\scan.md")
        );

        let locked = tasks.lock().unwrap();
        let task = locked.get(scan_id).unwrap();
        assert!(task.process_id.is_none());
        assert_eq!(task.status.phase, "completed");
    }
    #[test]
    fn complete_scan_task_makes_report_available_with_completed_status() {
        let scan_id = "scan-atomic";
        let tasks: TaskMap = Arc::new(Mutex::new(HashMap::new()));
        let status = ScanStatus {
            scan_id: scan_id.to_string(),
            phase: "running".to_string(),
            percent: 76,
            message: "building report".to_string(),
            started_at: Some("2026-06-21T00:00:00Z".to_string()),
            completed_at: None,
            error: None,
            markdown_report_path: None,
            json_report_path: None,
        };
        let report = ScanReport {
            schema_version: "0.1.0".to_string(),
            scan_id: scan_id.to_string(),
            created_at: "2026-06-21T00:00:01Z".to_string(),
            drive: "C:".to_string(),
            is_elevated: false,
            skipped_reparse_points: 0,
            scan_errors: Vec::new(),
            privacy: PrivacyLedger { uploaded: false },
            recommendations: Vec::new(),
            markdown_report_path: "C:\\reports\\scan.md".to_string(),
            json_report_path: "C:\\reports\\scan.json".to_string(),
        };

        {
            let mut locked = tasks.lock().unwrap();
            locked.insert(
                scan_id.to_string(),
                ScanTask {
                    status,
                    report: None,
                    process_id: Some(123),
                },
            );
        }

        let completed = complete_scan_task(&tasks, scan_id, report)
            .unwrap()
            .unwrap();
        assert_eq!(completed.phase, "completed");
        assert_eq!(completed.percent, 100);
        assert_eq!(
            completed.markdown_report_path.as_deref(),
            Some("C:\\reports\\scan.md")
        );
        assert_eq!(
            completed.json_report_path.as_deref(),
            Some("C:\\reports\\scan.json")
        );

        let locked = tasks.lock().unwrap();
        let task = locked.get(scan_id).unwrap();
        assert!(task.report.is_some());
        assert_eq!(task.status.phase, "completed");
        assert_eq!(
            task.status.markdown_report_path.as_deref(),
            Some("C:\\reports\\scan.md")
        );
        assert!(task.process_id.is_none());
    }

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
    #[test]
    fn scanner_progress_parser_accepts_known_markers() {
        let (percent, message) = parse_scanner_progress("[WCDCA_PROGRESS] 65|LARGE_FILES").unwrap();

        assert_eq!(percent, 65);
        assert_eq!(message, "正在查找大文件。");
    }

    #[test]
    fn scanner_progress_parser_clamps_percent_and_ignores_noise() {
        let (percent, message) =
            parse_scanner_progress("[WCDCA_PROGRESS] 120|DRILLDOWN:C:\\Users\\me").unwrap();

        assert_eq!(percent, 99);
        assert!(message.contains("C:\\Users\\me"));
        assert!(parse_scanner_progress("[OK] Report written").is_none());
    }

    #[test]
    fn resource_scanner_candidates_prefer_script_folder_resource() {
        let root = Path::new("C:\\Program Files\\WindowsCDriveCleanupAdvisor");
        let candidates = resource_scanner_candidates(root);

        assert_eq!(
            candidates,
            vec![
                root.join("scripts").join("Scan-CDriveCleanupAdvisor.ps1"),
                root.join("Scan-CDriveCleanupAdvisor.ps1"),
            ]
        );
    }
}
