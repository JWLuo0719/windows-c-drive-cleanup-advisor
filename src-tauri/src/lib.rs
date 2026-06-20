use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs,
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

#[tauri::command]
fn start_scan(options: ScanOptions, app: AppHandle, state: State<'_, AppState>) -> Result<String, AppError> {
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
        let mut tasks = state.tasks.lock().map_err(|_| AppError::Message("扫描任务状态暂不可用。".to_string()))?;
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
        if let Err(err) = run_scan_worker(app_for_thread.clone(), tasks.clone(), scan_id_for_thread.clone(), drive, top_count, large_file_mb) {
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
    let tasks = state.tasks.lock().map_err(|_| AppError::Message("扫描任务状态暂不可用。".to_string()))?;
    let task = tasks
        .get(&scan_id)
        .ok_or_else(|| AppError::Message(format!("未知扫描 ID: {scan_id}")))?;
    Ok(task.status.clone())
}

#[tauri::command]
fn cancel_scan(scan_id: String, app: AppHandle, state: State<'_, AppState>) -> Result<ScanStatus, AppError> {
    let process_id = {
        let tasks = state.tasks.lock().map_err(|_| AppError::Message("扫描任务状态暂不可用。".to_string()))?;
        let task = tasks
            .get(&scan_id)
            .ok_or_else(|| AppError::Message(format!("未知扫描 ID: {scan_id}")))?;
        task.process_id
    };

    if let Some(pid) = process_id {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    let status = update_task_status(
        &state.tasks,
        &scan_id,
        "cancelled",
        100,
        "扫描已取消。本次未执行任何清理动作。",
        None,
        None,
        None,
    )
    .ok_or_else(|| AppError::Message(format!("未知扫描 ID: {scan_id}")))?;
    emit_progress(&app, &status);
    Ok(status)
}

#[tauri::command]
fn get_scan_report(scan_id: String, state: State<'_, AppState>) -> Result<ScanReport, AppError> {
    let tasks = state.tasks.lock().map_err(|_| AppError::Message("扫描任务状态暂不可用。".to_string()))?;
    let task = tasks
        .get(&scan_id)
        .ok_or_else(|| AppError::Message(format!("未知扫描 ID: {scan_id}")))?;
    task.report
        .clone()
        .ok_or_else(|| AppError::Message("报告尚未生成完成。".to_string()))
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
    let child = Command::new(&shell)
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
        let mut locked = tasks.lock().map_err(|_| AppError::Message("扫描任务状态暂不可用。".to_string()))?;
        if let Some(task) = locked.get_mut(&scan_id) {
            task.process_id = Some(child.id());
        }
    }

    if let Some(status) = update_task_status(
        &tasks,
        &scan_id,
        "running",
        25,
        "正在扫描真实本地路径，并跳过重解析目录。",
        None,
        None,
        None,
    ) {
        emit_progress(&app, &status);
    }

    let output = child.wait_with_output()?;
    let was_cancelled = current_phase(&tasks, &scan_id).as_deref() == Some("cancelled");
    if was_cancelled {
        return Ok(());
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let detail = if stderr.is_empty() { stdout } else { stderr };
        return Err(AppError::Message(format!("PowerShell 扫描器退出状态为 {}。{detail}", output.status)));
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
    let markdown_path = latest_file_with_extension(&output_dir, "md")
        .ok_or_else(|| AppError::Message("PowerShell 扫描器没有生成 Markdown 输出。".to_string()))?;
    let raw_json = fs::read_to_string(&raw_json_path)?;
    let raw: Value = serde_json::from_str(&raw_json)?;
    let enriched_json_path = output_dir.join(format!("scan-report-{scan_id}.json"));
    let report = build_scan_report(&scan_id, &drive, &raw, &markdown_path, &enriched_json_path)?;
    fs::write(&enriched_json_path, serde_json::to_string_pretty(&report)?)?;

    let completed = update_task_status(
        &tasks,
        &scan_id,
        "completed",
        100,
        "扫描完成。本次没有删除、移动或修改任何文件。",
        None,
        Some(markdown_path.display().to_string()),
        Some(enriched_json_path.display().to_string()),
    );

    {
        let mut locked = tasks.lock().map_err(|_| AppError::Message("扫描任务状态暂不可用。".to_string()))?;
        if let Some(task) = locked.get_mut(&scan_id) {
            task.report = Some(report);
            task.process_id = None;
        }
    }

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
        return Err(AppError::Message("盘符必须是单个 Windows 驱动器字母。".to_string()));
    }
    Ok(letter.to_ascii_uppercase().to_string())
}

fn resolve_scanner_script(app: &AppHandle) -> Result<PathBuf, AppError> {
    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("scripts")
        .join("Scan-CDriveCleanupAdvisor.ps1");
    if dev_path.exists() {
        return Ok(dev_path);
    }

    let resource_dir = app.path().resource_dir().map_err(|err| AppError::Message(err.to_string()))?;
    let candidates = [
        resource_dir.join("scripts").join("Scan-CDriveCleanupAdvisor.ps1"),
        resource_dir.join("Scan-CDriveCleanupAdvisor.ps1"),
    ];
    candidates
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| AppError::Message("没有找到内置扫描脚本。".to_string()))
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
    collect_rows(raw.get("largeFiles"), &mut recommendations, &mut seen, "heuristic");

    recommendations.sort_by(|a, b| b.size_gb.partial_cmp(&a.size_gb).unwrap_or(std::cmp::Ordering::Equal));
    recommendations.truncate(80);

    Ok(ScanReport {
        schema_version: "0.1.0".to_string(),
        scan_id: scan_id.to_string(),
        created_at: Utc::now().to_rfc3339(),
        drive: format!("{drive}:"),
        is_elevated: raw.get("isAdmin").and_then(Value::as_bool).unwrap_or(false),
        skipped_reparse_points: count_skipped_reparse_points(raw),
        scan_errors: Vec::new(),
        privacy: PrivacyLedger { uploaded: false },
        recommendations,
        markdown_report_path: markdown_path.display().to_string(),
        json_report_path: enriched_json_path.display().to_string(),
    })
}

fn collect_rows(source: Option<&Value>, recommendations: &mut Vec<Recommendation>, seen: &mut HashSet<String>, source_name: &str) {
    let Some(rows) = source.and_then(Value::as_array) else {
        return;
    };

    for row in rows {
        let Some(path) = row.get("Path").or_else(|| row.get("FullName")).and_then(Value::as_str) else {
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
            reason: "这是由 Windows 管理的位置。手动删除可能影响系统修复、更新、启动或回滚能力。".to_string(),
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
        reason: "这是用户可见或类似项目的数据，可能包含下载、文档、媒体、源码或导出文件。".to_string(),
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

fn current_phase(tasks: &TaskMap, scan_id: &str) -> Option<String> {
    let locked = tasks.lock().ok()?;
    locked.get(scan_id).map(|task| task.status.phase.clone())
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
            get_scan_report
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}
