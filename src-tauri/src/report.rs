use crate::classify::{
    aggregate_repeated_cache_recommendations, classify_recommendation,
    compare_recommendations_for_review, Recommendation,
};
use crate::errors::AppError;
use crate::tasks::{is_terminal_phase, prune_task_history, ScanStatus, ScanTask, TaskMap};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};
use tauri::{AppHandle, Manager};

/// `reports` 目录下保留的历史报告目录上限（含当前扫描目录）。
pub(crate) const MAX_REPORT_HISTORY: usize = 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScanReport {
    pub(crate) schema_version: String,
    pub(crate) scan_id: String,
    pub(crate) created_at: String,
    pub(crate) drive: String,
    pub(crate) is_elevated: bool,
    pub(crate) skipped_reparse_points: u64,
    pub(crate) scan_errors: Vec<String>,
    pub(crate) privacy: PrivacyLedger,
    pub(crate) recommendations: Vec<Recommendation>,
    pub(crate) markdown_report_path: String,
    pub(crate) json_report_path: String,
    /// Phase 3 additive：原始顶层目录行（treemap/对比数据源；老报告缺省为空）。
    #[serde(default)]
    pub(crate) top_rows: Vec<DirRowView>,
    /// Phase 3 additive：deep 深挖行（root → 子项行）。
    #[serde(default)]
    pub(crate) drilldowns: Vec<DrilldownView>,
    /// Phase 3 additive：大文件行。
    #[serde(default)]
    pub(crate) large_files: Vec<LargeFileView>,
}

/// 目录行视图（透传自扫描内核 raw JSON 的 PS 属性语义）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DirRowView {
    pub(crate) path: String,
    pub(crate) size_gb: f64,
    pub(crate) files: u64,
    pub(crate) dirs: u64,
    pub(crate) skipped_reparse_points: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DrilldownView {
    pub(crate) root: String,
    pub(crate) rows: Vec<DirRowView>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LargeFileView {
    pub(crate) path: String,
    pub(crate) size_gb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PrivacyLedger {
    pub(crate) uploaded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RevealMode {
    SelectFile,
    OpenFolder,
}

pub(crate) fn resolve_report_dir(app: &AppHandle, scan_id: &str) -> Result<PathBuf, AppError> {
    Ok(resolve_reports_root(app)?.join(scan_id))
}

pub(crate) fn resolve_reports_root(app: &AppHandle) -> Result<PathBuf, AppError> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|err| AppError::Message(err.to_string()))?;
    Ok(base.join("reports"))
}

pub(crate) fn latest_enriched_report_path(app: &AppHandle) -> Result<Option<PathBuf>, AppError> {
    latest_enriched_report_path_from_root(&resolve_reports_root(app)?)
}

pub(crate) fn latest_enriched_report_path_from_root(
    reports_root: &Path,
) -> Result<Option<PathBuf>, AppError> {
    if !reports_root.exists() {
        return Ok(None);
    }

    // 优先读取 latest 索引，避免每次载入都全量遍历所有报告目录。
    if let Some(scan_id) = read_latest_report_index(reports_root) {
        let candidate = reports_root
            .join(&scan_id)
            .join(format!("scan-report-{scan_id}.json"));
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
    }

    // 索引缺失或失效（目录被淘汰、文件被移动）时回退全量扫描。

    let mut candidates = Vec::new();
    for scan_dir in fs::read_dir(&reports_root)? {
        let scan_dir = scan_dir?;
        let scan_dir_path = scan_dir.path();
        if !scan_dir_path.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&scan_dir_path)? {
            let entry = entry?;
            let path = entry.path();
            let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if !file_name.starts_with("scan-report-") || !file_name.ends_with(".json") {
                continue;
            }
            let modified = fs::metadata(&path)?.modified()?;
            candidates.push((modified, path));
        }
    }

    candidates.sort_by_key(|(modified, _)| *modified);
    Ok(candidates.pop().map(|(_, path)| path))
}

fn latest_report_index_path(reports_root: &Path) -> PathBuf {
    reports_root.join("latest-report.txt")
}

fn write_latest_report_index(reports_root: &Path, scan_id: &str) -> Result<(), AppError> {
    fs::write(latest_report_index_path(reports_root), format!("{scan_id}\n"))?;
    Ok(())
}

/// 更新最新报告索引；索引写失败不阻塞扫描完成，
/// 但必须尽力清掉陈旧索引——否则 load_latest_report 会命中旧索引
/// 而看不到刚完成的新报告。清理也失败时留下的旧索引是陈旧但有效的报告，
/// 下一次成功写索引即自愈。
pub(crate) fn update_latest_report_index(reports_root: &Path, scan_id: &str) {
    if write_latest_report_index(reports_root, scan_id).is_err() {
        let _ = fs::remove_file(latest_report_index_path(reports_root));
    }
}

fn read_latest_report_index(reports_root: &Path) -> Option<String> {
    let raw = fs::read_to_string(latest_report_index_path(reports_root)).ok()?;
    let scan_id = raw.trim();
    if scan_id.is_empty() {
        None
    } else {
        Some(scan_id.to_string())
    }
}

/// 目录内是否含增强报告文件（scan-report-*.json）。
/// 只有含增强报告的目录才占用报告保留名额；
/// 排队取消/中途失败扫描的残留目录没有报告，属于垃圾产物。
fn dir_has_enriched_report(dir: &Path) -> bool {
    fs::read_dir(dir)
        .map(|entries| {
            entries.flatten().any(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .map(|name| name.starts_with("scan-report-") && name.ends_with(".json"))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// reports 目录环形淘汰：删除超出保留上限的历史扫描目录。
/// 仅作用于 reports 根下的直接子目录（应用自有产物），不触碰用户文件与索引文件。
/// 保留名额只按含增强报告（scan-report-*.json）的目录计算；
/// 无报告的残留目录（排队取消/中途失败产物）不占名额，当前扫描之外一律清理——
/// 否则垃圾目录会挤掉真实报告，甚至在只有一份报告时把它误删。
pub(crate) fn prune_report_dirs(reports_root: &Path, keep: usize, current_scan_id: &str) {
    let Ok(entries) = fs::read_dir(reports_root) else {
        return;
    };
    let mut dirs: Vec<(SystemTime, String, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        if name == current_scan_id {
            continue;
        }
        if !dir_has_enriched_report(&path) {
            // 无报告的残留目录：不占名额直接清理。
            let _ = fs::remove_dir_all(&path);
            continue;
        }
        let Ok(modified) = fs::metadata(&path).and_then(|meta| meta.modified()) else {
            continue;
        };
        dirs.push((modified, name, path));
    }
    for path in select_report_dirs_to_remove(dirs, keep) {
        let _ = fs::remove_dir_all(path);
    }
}

/// 纯选择逻辑：对含报告的历史目录按（修改时间, 目录名）从旧到新排序，
/// 保留最新 keep-1 个（keep 名额含当前扫描目录），返回应淘汰的路径。
fn select_report_dirs_to_remove(
    dirs: Vec<(SystemTime, String, PathBuf)>,
    keep: usize,
) -> Vec<PathBuf> {
    let mut sorted = dirs;
    sorted.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let keep_history = keep.saturating_sub(1);
    if sorted.len() <= keep_history {
        return Vec::new();
    }
    let remove_count = sorted.len() - keep_history;
    sorted
        .into_iter()
        .take(remove_count)
        .map(|(_, _, path)| path)
        .collect()
}

pub(crate) fn read_enriched_report(path: &Path) -> Result<ScanReport, AppError> {
    let raw_json = fs::read_to_string(path)?;
    let value = parse_scanner_json(&raw_json)?;
    serde_json::from_value(value).map_err(AppError::Json)
}

/// 历史报告摘要（列表与对比入口；不展开建议明细）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReportSummary {
    pub(crate) scan_id: String,
    pub(crate) created_at: String,
    pub(crate) drive: String,
    pub(crate) recommendation_count: usize,
    pub(crate) total_size_gb: f64,
    pub(crate) scan_error_count: usize,
    pub(crate) is_latest: bool,
}

/// 校验报告 ID 必须是纯 UUID（报告目录名），拒绝路径穿越类输入。
pub(crate) fn parse_report_scan_id(scan_id: &str) -> Result<String, AppError> {
    uuid::Uuid::parse_str(scan_id)
        .map(|id| id.to_string())
        .map_err(|_| AppError::Message("无效的报告 ID。".to_string()))
}

/// 指定 ID 的增强报告路径（scan_id 必须先过 parse_report_scan_id）。
pub(crate) fn enriched_report_path_by_id(reports_root: &Path, scan_id: &str) -> Option<PathBuf> {
    let candidate = reports_root
        .join(scan_id)
        .join(format!("scan-report-{scan_id}.json"));
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

/// 历史报告摘要列表：按创建时间降序，最新一条标记 is_latest。
/// 单条报告读取失败（文件被外部破坏/写入中断）跳过，不阻塞其余条目。
pub(crate) fn list_report_summaries_from_root(
    reports_root: &Path,
) -> Result<Vec<ReportSummary>, AppError> {
    if !reports_root.exists() {
        return Ok(Vec::new());
    }
    let latest_id = read_latest_report_index(reports_root);
    let mut summaries = Vec::new();
    for entry in fs::read_dir(reports_root)?.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        // 只认 UUID 命名的扫描目录，其余目录（索引文件同级的杂项目录）不进历史。
        if parse_report_scan_id(&name).is_err() {
            continue;
        }
        let Some(report_path) = enriched_report_path_by_id(reports_root, &name) else {
            continue;
        };
        let Ok(report) = read_enriched_report(&report_path) else {
            continue;
        };
        let total_size_gb = report.recommendations.iter().map(|item| item.size_gb).sum();
        summaries.push(ReportSummary {
            scan_id: report.scan_id.clone(),
            created_at: report.created_at.clone(),
            drive: report.drive.clone(),
            recommendation_count: report.recommendations.len(),
            total_size_gb,
            scan_error_count: report.scan_errors.len(),
            is_latest: latest_id.as_deref() == Some(name.as_str()),
        });
    }
    summaries.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(summaries)
}

pub(crate) fn register_completed_report(
    tasks: &TaskMap,
    report: ScanReport,
) -> Result<(), AppError> {
    let scan_id = report.scan_id.clone();
    let status = ScanStatus {
        scan_id: scan_id.clone(),
        phase: "completed".to_string(),
        percent: 100,
        message: "已载入最近一次本地报告。本次没有执行任何清理动作。".to_string(),
        started_at: None,
        completed_at: Some(report.created_at.clone()),
        error: None,
        markdown_report_path: Some(report.markdown_report_path.clone()),
        json_report_path: Some(report.json_report_path.clone()),
    };

    let mut locked = tasks
        .lock()
        .map_err(|_| AppError::Message("扫描任务状态暂不可用。".to_string()))?;
    if let Some(existing) = locked.get(&scan_id) {
        // 进行中的同名扫描优先：不被“载入最近报告”覆盖。
        if !is_terminal_phase(&existing.status.phase) {
            return Ok(());
        }
    }
    locked.insert(
        scan_id,
        ScanTask {
            status,
            report: Some(report),
            process_id: None,
        },
    );
    prune_task_history(&mut locked);
    Ok(())
}

pub(crate) fn latest_file_with_extension(dir: &Path, extension: &str) -> Option<PathBuf> {
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

pub(crate) fn parse_scanner_json(raw_json: &str) -> Result<Value, AppError> {
    let normalized = raw_json.trim_start_matches('\u{feff}');
    serde_json::from_str(normalized).map_err(AppError::Json)
}

pub(crate) fn build_scan_report(
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

    recommendations = aggregate_repeated_cache_recommendations(recommendations);
    recommendations.sort_by(compare_recommendations_for_review);
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
        top_rows: collect_dir_views(raw.get("top")),
        drilldowns: collect_drilldown_views(raw),
        large_files: collect_large_file_views(raw.get("largeFiles")),
    })
}

/// 透传目录行（PS 属性语义：Path/SizeGB/Files/Dirs/SkippedReparsePoints）。
fn collect_dir_views(source: Option<&Value>) -> Vec<DirRowView> {
    let Some(rows) = source.and_then(Value::as_array) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let path = row
                .get("Path")
                .or_else(|| row.get("FullName"))
                .and_then(Value::as_str)?
                .to_string();
            Some(DirRowView {
                path,
                size_gb: row.get("SizeGB").and_then(Value::as_f64).unwrap_or(0.0),
                files: row.get("Files").and_then(Value::as_u64).unwrap_or(0),
                dirs: row.get("Dirs").and_then(Value::as_u64).unwrap_or(0),
                skipped_reparse_points: row
                    .get("SkippedReparsePoints")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            })
        })
        .collect()
}

fn collect_drilldown_views(raw: &Value) -> Vec<DrilldownView> {
    let Some(drilldowns) = raw.get("drilldowns").and_then(Value::as_object) else {
        return Vec::new();
    };
    drilldowns
        .iter()
        .map(|(root, rows)| DrilldownView {
            root: root.clone(),
            rows: collect_dir_views(Some(rows)),
        })
        .collect()
}

fn collect_large_file_views(source: Option<&Value>) -> Vec<LargeFileView> {
    let Some(rows) = source.and_then(Value::as_array) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let path = row
                .get("Path")
                .or_else(|| row.get("FullName"))
                .and_then(Value::as_str)?
                .to_string();
            Some(LargeFileView {
                path,
                size_gb: row.get("SizeGB").and_then(Value::as_f64).unwrap_or(0.0),
            })
        })
        .collect()
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

#[cfg(test)]
pub(crate) fn test_scan_report(markdown_report_path: &str, json_report_path: &str) -> ScanReport {
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
        top_rows: Vec::new(),
        drilldowns: Vec::new(),
        large_files: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tasks::{insert_queued_scan_task, test_scan_status};
    use serde_json::json;
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
        thread,
        time::Duration,
    };
    use uuid::Uuid;

    #[test]
    fn repeated_cache_files_aggregate_to_directory_candidate() {
        let raw = json!({
            "isAdmin": false,
            "top": [],
            "largeFiles": [
                { "FullName": "C:\\Users\\me\\AppData\\Local\\NVIDIA\\DXCache\\a.bin", "SizeGB": 1.2 },
                { "FullName": "C:\\Users\\me\\AppData\\Local\\NVIDIA\\DXCache\\b.bin", "SizeGB": 2.3 },
                { "FullName": "C:\\Users\\me\\Downloads\\archive.zip", "SizeGB": 3.4 }
            ],
            "scanErrors": []
        });

        let report = build_scan_report(
            "scan-cache",
            "C",
            &raw,
            Path::new("report.md"),
            Path::new("report.json"),
        )
        .unwrap();

        let paths = report
            .recommendations
            .iter()
            .map(|item| item.path.as_str())
            .collect::<Vec<_>>();
        assert!(paths.contains(&"C:\\Users\\me\\AppData\\Local\\NVIDIA\\DXCache"));
        assert!(!paths.contains(&"C:\\Users\\me\\AppData\\Local\\NVIDIA\\DXCache\\a.bin"));
        assert!(!paths.contains(&"C:\\Users\\me\\AppData\\Local\\NVIDIA\\DXCache\\b.bin"));
        assert!(paths.contains(&"C:\\Users\\me\\Downloads\\archive.zip"));

        let aggregated = report
            .recommendations
            .iter()
            .find(|item| item.path == "C:\\Users\\me\\AppData\\Local\\NVIDIA\\DXCache")
            .unwrap();
        assert_eq!(aggregated.category, "low-risk-cache");
        assert_eq!(aggregated.source, "heuristic");
        assert!((aggregated.size_gb - 3.5).abs() < f64::EPSILON);
        assert!(aggregated.reason.contains("2"));
        assert!(!aggregated.cleanable);
    }

    #[test]
    fn existing_cache_directory_suppresses_child_file_rows() {
        let raw = json!({
            "isAdmin": false,
            "top": [],
            "drilldowns": {
                "AppData": [
                    { "Path": "C:\\Users\\me\\AppData\\Local\\NVIDIA\\DXCache", "SizeGB": 8.0 }
                ]
            },
            "largeFiles": [
                { "FullName": "C:\\Users\\me\\AppData\\Local\\NVIDIA\\DXCache\\a.bin", "SizeGB": 1.2 },
                { "FullName": "C:\\Users\\me\\AppData\\Local\\NVIDIA\\DXCache\\b.bin", "SizeGB": 2.3 }
            ],
            "scanErrors": []
        });

        let report = build_scan_report(
            "scan-cache-root",
            "C",
            &raw,
            Path::new("report.md"),
            Path::new("report.json"),
        )
        .unwrap();

        let cache_items = report
            .recommendations
            .iter()
            .filter(|item| item.path.contains("DXCache"))
            .collect::<Vec<_>>();
        assert_eq!(cache_items.len(), 1);
        assert_eq!(
            cache_items[0].path,
            "C:\\Users\\me\\AppData\\Local\\NVIDIA\\DXCache"
        );
        assert_eq!(cache_items[0].source, "scanner");
        assert!((cache_items[0].size_gb - 8.0).abs() < f64::EPSILON);
    }

    #[test]
    fn single_cache_file_stays_as_a_file_candidate() {
        let raw = json!({
            "isAdmin": false,
            "top": [],
            "largeFiles": [
                { "FullName": "C:\\Users\\me\\AppData\\Local\\NVIDIA\\DXCache\\only.bin", "SizeGB": 1.2 }
            ],
            "scanErrors": []
        });

        let report = build_scan_report(
            "scan-single-cache-file",
            "C",
            &raw,
            Path::new("report.md"),
            Path::new("report.json"),
        )
        .unwrap();

        assert!(report.recommendations.iter().any(|item| {
            item.path == "C:\\Users\\me\\AppData\\Local\\NVIDIA\\DXCache\\only.bin"
                && item.category == "low-risk-cache"
                && !item.cleanable
        }));
        assert!(!report
            .recommendations
            .iter()
            .any(|item| item.path == "C:\\Users\\me\\AppData\\Local\\NVIDIA\\DXCache"));
    }

    #[test]
    fn repeated_cache_files_in_distinct_directories_stay_separate() {
        let raw = json!({
            "isAdmin": false,
            "top": [],
            "largeFiles": [
                { "FullName": "C:\\Users\\me\\AppData\\Local\\NVIDIA\\DXCache\\a.bin", "SizeGB": 1.0 },
                { "FullName": "C:\\Users\\me\\AppData\\Local\\NVIDIA\\DXCache\\b.bin", "SizeGB": 2.0 },
                { "FullName": "C:\\Users\\me\\AppData\\Local\\App\\GPUCache\\a.bin", "SizeGB": 3.0 },
                { "FullName": "C:\\Users\\me\\AppData\\Local\\App\\GPUCache\\b.bin", "SizeGB": 4.0 }
            ],
            "scanErrors": []
        });

        let report = build_scan_report(
            "scan-distinct-cache-directories",
            "C",
            &raw,
            Path::new("report.md"),
            Path::new("report.json"),
        )
        .unwrap();

        let aggregates = report
            .recommendations
            .iter()
            .filter(|item| item.cleanup_method.as_deref() == Some("manual-cache-review"))
            .collect::<Vec<_>>();
        assert_eq!(aggregates.len(), 2);
        assert!(aggregates.iter().any(|item| {
            item.path == "C:\\Users\\me\\AppData\\Local\\NVIDIA\\DXCache"
                && (item.size_gb - 3.0).abs() < f64::EPSILON
        }));
        assert!(aggregates.iter().any(|item| {
            item.path == "C:\\Users\\me\\AppData\\Local\\App\\GPUCache"
                && (item.size_gb - 7.0).abs() < f64::EPSILON
        }));
    }

    #[test]
    fn protected_windows_cache_paths_never_become_low_risk() {
        let raw = json!({
            "isAdmin": false,
            "top": [],
            "largeFiles": [
                { "FullName": "C:\\Windows\\System32\\Cache\\a.bin", "SizeGB": 1.0 },
                { "FullName": "C:\\Windows\\System32\\Cache\\b.bin", "SizeGB": 2.0 }
            ],
            "scanErrors": []
        });

        let report = build_scan_report(
            "scan-protected-cache",
            "C",
            &raw,
            Path::new("report.md"),
            Path::new("report.json"),
        )
        .unwrap();

        let windows_items = report
            .recommendations
            .iter()
            .filter(|item| item.path.starts_with("C:\\Windows\\System32"))
            .collect::<Vec<_>>();
        assert_eq!(windows_items.len(), 2);
        assert!(windows_items.iter().all(|item| {
            item.category == "system-managed" && item.risk == "blocked" && !item.cleanable
        }));
    }

    #[test]
    fn scan_report_passes_through_tree_rows_for_discovery_layer() {
        let raw = json!({
            "isAdmin": false,
            "top": [
                {
                    "Path": "C:\\Users\\me\\Downloads",
                    "SizeGB": 5.25,
                    "Files": 12,
                    "Dirs": 3,
                    "SkippedReparsePoints": 2
                }
            ],
            "drilldowns": {
                "C:\\Users\\me": [
                    {
                        "Path": "C:\\Users\\me\\Downloads",
                        "SizeGB": 5.25,
                        "Files": 12,
                        "Dirs": 3,
                        "SkippedReparsePoints": 2
                    }
                ]
            },
            "largeFiles": [
                { "Path": "C:\\Users\\me\\Downloads\\big.iso", "SizeGB": 4.5 }
            ],
            "scanErrors": []
        });

        let report = build_scan_report(
            "scan-tree",
            "C",
            &raw,
            Path::new("report.md"),
            Path::new("report.json"),
        )
        .unwrap();

        assert_eq!(report.top_rows.len(), 1);
        assert_eq!(report.top_rows[0].path, "C:\\Users\\me\\Downloads");
        assert!((report.top_rows[0].size_gb - 5.25).abs() < f64::EPSILON);
        assert_eq!(report.top_rows[0].files, 12);
        assert_eq!(report.top_rows[0].dirs, 3);
        assert_eq!(report.top_rows[0].skipped_reparse_points, 2);

        assert_eq!(report.drilldowns.len(), 1);
        assert_eq!(report.drilldowns[0].root, "C:\\Users\\me");
        assert_eq!(report.drilldowns[0].rows.len(), 1);
        assert_eq!(report.drilldowns[0].rows[0].files, 12);

        assert_eq!(report.large_files.len(), 1);
        assert_eq!(report.large_files[0].path, "C:\\Users\\me\\Downloads\\big.iso");
        assert!((report.large_files[0].size_gb - 4.5).abs() < f64::EPSILON);

        // 老报告（无新字段）反序列化回退为空列表。
        let legacy = r#"{"schemaVersion":"0.1.0","scanId":"s","createdAt":"t","drive":"C:","isElevated":false,"skippedReparsePoints":0,"scanErrors":[],"privacy":{"uploaded":false},"recommendations":[],"markdownReportPath":"m","jsonReportPath":"j"}"#;
        let parsed: ScanReport = serde_json::from_str(legacy).unwrap();
        assert!(parsed.top_rows.is_empty());
        assert!(parsed.drilldowns.is_empty());
        assert!(parsed.large_files.is_empty());
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
    fn parse_scanner_json_accepts_utf8_bom() {
        let raw = "\u{feff}{\"drive\":\"C:\\\\\",\"top\":[]}";
        let parsed = parse_scanner_json(raw).unwrap();

        assert_eq!(parsed.get("drive").and_then(Value::as_str), Some("C:\\"));
    }

    #[test]
    fn read_enriched_report_and_registers_completed_task() {
        let temp_root =
            std::env::temp_dir().join(format!("wcdca-report-read-{}", Uuid::new_v4()));
        fs::create_dir_all(&temp_root).unwrap();
        let markdown_path = temp_root.join("scan.md");
        let json_path = temp_root.join("scan-report-scan-latest.json");
        fs::write(&markdown_path, "# report").unwrap();

        let report_json = json!({
            "schemaVersion": "0.1.0",
            "scanId": "scan-latest",
            "createdAt": "2026-06-30T00:00:00Z",
            "drive": "C:",
            "isElevated": false,
            "skippedReparsePoints": 7,
            "scanErrors": [],
            "privacy": { "uploaded": false },
            "recommendations": [],
            "markdownReportPath": markdown_path.display().to_string(),
            "jsonReportPath": json_path.display().to_string()
        });
        fs::write(&json_path, serde_json::to_string_pretty(&report_json).unwrap()).unwrap();

        let report = read_enriched_report(&json_path).unwrap();
        assert_eq!(report.scan_id, "scan-latest");
        assert_eq!(report.skipped_reparse_points, 7);

        let tasks = Arc::new(Mutex::new(HashMap::new()));
        register_completed_report(&tasks, report).unwrap();
        let locked = tasks.lock().unwrap();
        let task = locked.get("scan-latest").unwrap();
        assert_eq!(task.status.phase, "completed");
        assert_eq!(task.status.percent, 100);
        assert!(task.report.is_some());

        fs::remove_dir_all(temp_root).unwrap();
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
    fn register_completed_report_does_not_overwrite_active_scan() {
        let scan_id = "scan-live";
        let tasks: TaskMap = Arc::new(Mutex::new(HashMap::new()));
        insert_queued_scan_task(&tasks, scan_id, test_scan_status(scan_id, "running")).unwrap();

        let report = test_scan_report("C:\\reports\\scan.md", "C:\\reports\\scan.json");
        register_completed_report(&tasks, report).unwrap();

        let locked = tasks.lock().unwrap();
        assert_eq!(locked.get(scan_id).unwrap().status.phase, "running");
        assert!(locked.get(scan_id).unwrap().report.is_none());
    }

    #[test]
    fn select_report_dirs_to_remove_keeps_newest_history() {
        let epoch = SystemTime::UNIX_EPOCH;
        let dirs: Vec<(SystemTime, String, PathBuf)> = (0..5)
            .map(|index| {
                (
                    epoch + Duration::from_secs(index),
                    format!("dir-{index}"),
                    PathBuf::from(format!("C:\\reports\\dir-{index}")),
                )
            })
            .collect();

        // keep=3：当前扫描目录之外保留最新 2 个历史目录（dir-3/dir-4），淘汰 dir-0..2。
        let removed = select_report_dirs_to_remove(dirs, 3);
        assert_eq!(
            removed,
            vec![
                PathBuf::from("C:\\reports\\dir-0"),
                PathBuf::from("C:\\reports\\dir-1"),
                PathBuf::from("C:\\reports\\dir-2"),
            ]
        );
    }

    #[test]
    fn prune_report_dirs_removes_garbage_keeps_current_and_index() {
        let root = std::env::temp_dir().join(format!("wcdca-prune-{}", Uuid::new_v4()));
        fs::create_dir_all(root.join("current-scan")).unwrap();
        for index in 0..5 {
            let dir = root.join(format!("dir-{index}"));
            fs::create_dir_all(&dir).unwrap();
            // 无 scan-report 的残留目录（排队取消/中途失败产物）
            fs::write(dir.join("c-drive-cleanup-advisor-raw.txt"), "x").unwrap();
            thread::sleep(Duration::from_millis(5));
        }
        fs::write(root.join("latest-report.txt"), "dir-4\n").unwrap();

        prune_report_dirs(&root, 3, "current-scan");

        // 垃圾目录不占名额：全部清理；当前扫描目录与索引文件保留。
        for index in 0..5 {
            assert!(!root.join(format!("dir-{index}")).exists());
        }
        assert!(root.join("current-scan").exists());
        assert!(root.join("latest-report.txt").exists());

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn prune_report_dirs_keeps_real_report_when_garbage_dirs_overflow_quota() {
        let root = std::env::temp_dir().join(format!("wcdca-prune-real-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        // 唯一的真实报告目录（最旧）。回归保护：垃圾目录按目录计数挤占
        // keep-1=19 名额时，会把这份唯一报告连同索引目标一起误删。
        let real = root.join("scan-real");
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("scan-report-scan-real.json"), "{}").unwrap();
        thread::sleep(Duration::from_millis(5));
        for index in 0..19 {
            fs::create_dir_all(root.join(format!("garbage-{index}"))).unwrap();
            thread::sleep(Duration::from_millis(5));
        }

        prune_report_dirs(&root, 20, "current-scan");

        assert!(real.exists(), "唯一真实报告不得被垃圾目录挤掉");
        assert!(real.join("scan-report-scan-real.json").exists());
        for index in 0..19 {
            assert!(!root.join(format!("garbage-{index}")).exists());
        }

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn update_latest_report_index_write_failure_leaves_no_trusted_stale_index() {
        let root = std::env::temp_dir().join(format!("wcdca-index-fail-{}", Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        // 用同名目录占据索引路径制造写失败
        fs::create_dir_all(root.join("latest-report.txt")).unwrap();

        update_latest_report_index(&root, "scan-new");

        // 写失败路径不得抛错，也不得留下可被读取的陈旧索引内容
        assert!(read_latest_report_index(&root).is_none());

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn latest_report_index_roundtrip_and_fallback() {
        let root = std::env::temp_dir().join(format!("wcdca-index-{}", Uuid::new_v4()));
        let scan_id = "scan-index";
        let scan_dir = root.join(scan_id);
        fs::create_dir_all(&scan_dir).unwrap();

        // 无索引、无报告 → None
        assert!(latest_enriched_report_path_from_root(&root).unwrap().is_none());

        // 索引指向存在的增强报告 → 直接命中
        let report_path = scan_dir.join(format!("scan-report-{scan_id}.json"));
        fs::write(&report_path, "{}").unwrap();
        write_latest_report_index(&root, scan_id).unwrap();
        assert_eq!(
            latest_enriched_report_path_from_root(&root).unwrap(),
            Some(report_path.clone())
        );

        // 索引失效（指向不存在的扫描）→ 回退全量扫描仍能找到报告
        write_latest_report_index(&root, "missing-scan").unwrap();
        assert_eq!(
            latest_enriched_report_path_from_root(&root).unwrap(),
            Some(report_path)
        );

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn parse_report_scan_id_accepts_uuid_and_rejects_traversal() {
        let id = Uuid::new_v4().to_string();
        assert_eq!(parse_report_scan_id(&id).unwrap(), id);
        assert!(parse_report_scan_id("../evil").is_err());
        assert!(parse_report_scan_id("sub/dir").is_err());
        assert!(parse_report_scan_id("").is_err());
        assert!(parse_report_scan_id("scan-index").is_err());
    }

    #[test]
    fn list_report_summaries_sorts_marks_latest_and_skips_broken_entries() {
        let root = std::env::temp_dir().join(format!("wcdca-hist-{}", Uuid::new_v4()));
        let older_id = Uuid::new_v4().to_string();
        let newer_id = Uuid::new_v4().to_string();

        let write_report = |scan_id: &str, created_at: &str, size_gb: f64, error_count: usize| {
            let dir = root.join(scan_id);
            fs::create_dir_all(&dir).unwrap();
            let value = json!({
                "schemaVersion": "0.1.0",
                "scanId": scan_id,
                "createdAt": created_at,
                "drive": "C:",
                "isElevated": false,
                "skippedReparsePoints": 0,
                "scanErrors": (0..error_count).map(|index| format!("denied-{index}")).collect::<Vec<_>>(),
                "privacy": { "uploaded": false },
                "recommendations": [{
                    "id": "rec-1",
                    "path": "C:\\Temp\\cache",
                    "sizeGb": size_gb,
                    "category": "low-risk-cache",
                    "risk": "low",
                    "confidence": 0.9,
                    "reason": "test",
                    "manualSteps": [],
                    "cleanable": false,
                    "requiresAppClosed": false,
                    "source": "scanner"
                }],
                "markdownReportPath": "report.md",
                "jsonReportPath": "report.json"
            });
            fs::write(
                dir.join(format!("scan-report-{scan_id}.json")),
                serde_json::to_string(&value).unwrap(),
            )
            .unwrap();
        };

        write_report(&older_id, "2026-09-20T00:00:00Z", 1.5, 0);
        write_report(&newer_id, "2026-09-25T00:00:00Z", 2.5, 2);

        // 坏 JSON 与非 UUID 目录都必须被跳过，且不影响其余条目。
        let broken_id = Uuid::new_v4().to_string();
        let broken_dir = root.join(&broken_id);
        fs::create_dir_all(&broken_dir).unwrap();
        fs::write(
            broken_dir.join(format!("scan-report-{broken_id}.json")),
            "{ not json",
        )
        .unwrap();
        fs::create_dir_all(root.join("not-a-uuid")).unwrap();

        write_latest_report_index(&root, &older_id).unwrap();

        let summaries = list_report_summaries_from_root(&root).unwrap();
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].scan_id, newer_id);
        assert_eq!(summaries[0].created_at, "2026-09-25T00:00:00Z");
        assert_eq!(summaries[0].recommendation_count, 1);
        assert!((summaries[0].total_size_gb - 2.5).abs() < f64::EPSILON);
        assert_eq!(summaries[0].scan_error_count, 2);
        assert!(!summaries[0].is_latest);
        assert_eq!(summaries[1].scan_id, older_id);
        assert!(summaries[1].is_latest);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn enriched_report_path_by_id_only_resolves_uuid_named_dirs() {
        let root = std::env::temp_dir().join(format!("wcdca-byid-{}", Uuid::new_v4()));
        let scan_id = Uuid::new_v4().to_string();
        let dir = root.join(&scan_id);
        fs::create_dir_all(&dir).unwrap();
        let report_path = dir.join(format!("scan-report-{scan_id}.json"));
        fs::write(&report_path, "{}").unwrap();

        assert_eq!(
            enriched_report_path_by_id(&root, &scan_id),
            Some(report_path)
        );
        assert_eq!(enriched_report_path_by_id(&root, "missing-id"), None);

        fs::remove_dir_all(&root).unwrap();
    }
}
