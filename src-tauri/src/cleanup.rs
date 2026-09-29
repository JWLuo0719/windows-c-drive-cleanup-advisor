//! Phase 5：v0.3 实验性低风险缓存清理（回收站优先）。
//!
//! 安全模型（与 AGENT.md「Non-Negotiable Safety Boundary」一致，写权限仅限本模块）：
//! - 入参只有 `reportId + candidateIds`；路径一律从已存储报告反查，UI 不得直传路径。
//! - plan-first：`plan_cleanup` 与 `execute_cleanup` 共用同一 `build_plan_with` 验证函数，
//!   执行在 build 之后逐项做删除时刻复判（classify blocklist 复判 + 路径三重校验）。
//! - 三重校验：路径规范化与存在性、句柄身份复检（GetFileInformationByHandle file id
//!   在原始路径与 canonical 路径上必须一致）、拒绝 reparse 目标（含父目录链）。
//! - v0.3 allowlist：仅 `category == "low-risk-cache"` 且 `cleanable == true` 的候选可执行。
//! - 回收站优先：SHFileOperationW `FO_DELETE | FOF_ALLOWUNDO`；执行前用 SHQueryRecycleBinW
//!   确认回收站可用（禁用即拒绝），并按磁盘剩余的 1/10 做保守容量预算。
//! - action-log：每项执行结果追加到 `reports\action-log.jsonl` 审计。
//!
//! 纯用户态权限：不请求提权、不修改设置；allowlist 系统类别的提权助手不在 v0.3 范围。

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::classify::classify_recommendation;
use crate::errors::AppError;
use crate::report::ScanReport;

/// 回收站保守预算系数：磁盘剩余的 1/10 视作可用回收站空间（不精确，宁可拒绝不可永久删除）。
const RECYCLE_BUDGET_DIVISOR: u64 = 10;
/// action-log 读取上限。
const ACTION_LOG_READ_LIMIT: usize = 200;
/// Windows 文件属性 REPARSE_POINT。
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

// ==== IPC 数据结构（camelCase 对齐前端） ====

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlanItem {
    pub(crate) id: String,
    pub(crate) path: String,
    pub(crate) size_gb: f64,
    pub(crate) category: String,
    pub(crate) cleanup_method: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlanRejection {
    pub(crate) id: String,
    pub(crate) path: String,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CleanupPlan {
    pub(crate) report_id: String,
    pub(crate) items: Vec<PlanItem>,
    pub(crate) rejected: Vec<PlanRejection>,
    pub(crate) total_size_gb: f64,
    /// 保守回收站预算（GB），仅用于展示；低于总量时对应项会被拒绝。
    pub(crate) recycle_budget_gb: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CleanupItemOutcome {
    pub(crate) id: String,
    pub(crate) path: String,
    pub(crate) size_gb: f64,
    /// "recycled" | "failed"
    pub(crate) status: String,
    pub(crate) message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CleanupOutcome {
    pub(crate) report_id: String,
    pub(crate) results: Vec<CleanupItemOutcome>,
    pub(crate) rejected: Vec<PlanRejection>,
    pub(crate) recycled_count: u32,
    pub(crate) failed_count: u32,
    /// 成功回收项的合计 GB。
    pub(crate) total_size_gb: f64,
    pub(crate) action_log_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActionLogEntry {
    pub(crate) ts: String,
    pub(crate) report_id: String,
    pub(crate) id: String,
    pub(crate) path: String,
    pub(crate) size_gb: f64,
    pub(crate) action: String,
    pub(crate) status: String,
    pub(crate) message: Option<String>,
}

// ==== 回收站探测（注入点，单测不触 WinAPI） ====

#[derive(Debug, Clone)]
pub(crate) struct RecycleContext {
    /// 保守预算字节数。可用性本身由 `probe_recycle_context` 的 Err 表达
    /// （SHQueryRecycleBinW 失败 = 回收站禁用，直接拒绝整次清理）。
    pub(crate) budget_bytes: u64,
}

/// 探测回收站可用性与磁盘保守预算。
/// `sample_path` 用于确定所在卷（报告盘符根下的任一路径）。
pub(crate) fn probe_recycle_context(sample_path: &Path) -> Result<RecycleContext, String> {
    use windows_sys::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};

    let mut info: SHQUERYRBINFO = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHQUERYRBINFO>() as u32;
    // null 根路径 = 查询全部驱动器汇总；回收站被组策略禁用时该 API 失败。
    let hr = unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut info) };
    if hr != 0 {
        return Err("回收站不可用或已被禁用，为避免永久删除已取消清理。".to_string());
    }

    let disk_free = disk_free_bytes(sample_path)
        .map_err(|message| format!("无法读取磁盘可用空间：{message}"))?;
    let budget = disk_free / RECYCLE_BUDGET_DIVISOR;
    Ok(RecycleContext { budget_bytes: budget })
}

fn disk_free_bytes(path: &Path) -> Result<u64, String> {
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    // 取路径的卷根（如 "C:\"）查询该卷剩余空间。
    let root = volume_root(path)?;
    let root_wide: Vec<u16> = root
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let mut available: u64 = 0;
    let mut total: u64 = 0;
    let mut total_free: u64 = 0;
    let ok = unsafe {
        GetDiskFreeSpaceExW(root_wide.as_ptr(), &mut available, &mut total, &mut total_free)
    };
    if ok == 0 {
        return Err("GetDiskFreeSpaceExW 失败".to_string());
    }
    Ok(available)
}

fn volume_root(path: &Path) -> Result<PathBuf, String> {
    let root = path
        .components()
        .find(|component| matches!(component, std::path::Component::Prefix(_)))
        .ok_or_else(|| "路径缺少卷前缀".to_string())?;
    let mut buf = PathBuf::from(root.as_os_str());
    buf.push("");
    Ok(buf)
}

// ==== 路径三重校验 ====

#[derive(Debug, Clone, PartialEq, Eq)]
struct PathIdentity {
    volume_serial: u32,
    file_index: u64,
}

fn open_path_identity(path: &Path) -> Result<PathIdentity, String> {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION};

    // 目录句柄需要 FILE_FLAG_BACKUP_SEMANTICS；对文件该 flag 无副作用。
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .map_err(|error| format!("无法打开路径：{error}"))?;
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, &mut info) };
    if ok == 0 {
        return Err("读取文件身份信息失败".to_string());
    }
    let file_index = ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64;
    Ok(PathIdentity {
        volume_serial: info.dwVolumeSerialNumber,
        file_index,
    })
}

fn is_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

/// 三重校验：
/// 1. 规范化与存在性：symlink_metadata 存在且不是 reparse；父目录链逐级拒绝 reparse；
/// 2. 句柄身份复检：原始路径与 canonical 路径分别打开，file id 必须一致；
/// 3. reparse 目标：canonical 结果再次拒绝 reparse。
/// 返回 canonical 路径供后续使用。
fn validate_path_identity(path: &Path) -> Result<PathBuf, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| format!("路径不可访问：{error}"))?;
    if metadata.file_type().is_symlink() {
        return Err("目标是符号链接，已拒绝。".to_string());
    }
    if is_reparse(&metadata) {
        return Err("目标是重解析点（junction/符号链接），已拒绝。".to_string());
    }

    // 父目录链：任一级是 reparse 都拒绝（经 junction 删除会命中非预期目标）。
    let mut cursor = path.parent();
    while let Some(parent) = cursor {
        if let Ok(parent_meta) = fs::symlink_metadata(parent) {
            if is_reparse(&parent_meta) {
                return Err(format!("父目录是重解析点：{}，已拒绝。", parent.display()));
            }
        }
        cursor = parent.parent();
    }

    let identity_raw = open_path_identity(path)?;
    let canonical = fs::canonicalize(path).map_err(|error| format!("路径规范化失败：{error}"))?;
    let canonical_meta =
        fs::symlink_metadata(&canonical).map_err(|error| format!("规范化路径不可访问：{error}"))?;
    if is_reparse(&canonical_meta) {
        return Err("规范化后指向重解析目标，已拒绝。".to_string());
    }
    let identity_canonical = open_path_identity(&canonical)?;
    if identity_raw != identity_canonical {
        return Err("路径身份复检失败（打开前后目标不一致），已拒绝。".to_string());
    }
    Ok(canonical)
}

/// 删除时刻的 blocklist 复判：重新跑分类器，系统托管/blocked 一律拒绝。
fn reclassify_gate(path: &str) -> Result<(), String> {
    let fresh = classify_recommendation(path, 0.0, "cleanup-recheck");
    if fresh.category == "system-managed" || fresh.risk == "blocked" {
        return Err(format!(
            "删除时刻复判不通过（{}），已拒绝。",
            fresh.category
        ));
    }
    if fresh.category != "low-risk-cache" {
        return Err(format!(
            "删除时刻复判为 {}，不在 v0.3 allowlist 内。",
            fresh.category
        ));
    }
    Ok(())
}

// ==== 计划构建（plan 与 execute 共用） ====

fn build_plan_with(
    report: &ScanReport,
    report_id: &str,
    candidate_ids: &[String],
    context: &RecycleContext,
) -> CleanupPlan {
    let mut items = Vec::new();
    let mut rejected = Vec::new();
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut accepted_bytes: u64 = 0;

    for raw_id in candidate_ids {
        let id = raw_id.trim().to_string();
        if id.is_empty() || !seen_ids.insert(id.clone()) {
            continue;
        }
        let Some(rec) = report.recommendations.iter().find(|item| item.id == id) else {
            rejected.push(PlanRejection {
                id: id.clone(),
                path: String::new(),
                reason: "候选不在报告中，已拒绝。".to_string(),
            });
            continue;
        };
        let reject = |reason: String| PlanRejection {
            id: id.clone(),
            path: rec.path.clone(),
            reason,
        };

        if !rec.cleanable {
            rejected.push(reject(
                rec.blocked_reason
                    .clone()
                    .unwrap_or_else(|| "该候选不允许自动清理。".to_string()),
            ));
            continue;
        }
        if rec.category != "low-risk-cache" {
            rejected.push(reject(format!(
                "v0.3 仅允许 low-risk-cache 候选，该项为 {}。",
                rec.category
            )));
            continue;
        }
        // 报告分类是扫描时的缓存：删除计划阶段重新复判（不信任报告字段）。
        if let Err(message) = reclassify_gate(&rec.path) {
            rejected.push(reject(message));
            continue;
        }
        let path = PathBuf::from(&rec.path);
        if let Err(message) = validate_path_identity(&path) {
            rejected.push(reject(message));
            continue;
        }

        let size_bytes = rec.size_gb.max(0.0) * 1024.0 * 1024.0 * 1024.0;
        let next_total = accepted_bytes + size_bytes.ceil() as u64;
        if next_total > context.budget_bytes {
            rejected.push(reject(
                "超出回收站保守预算（剩余空间不足可能导致永久删除），已拒绝。".to_string(),
            ));
            continue;
        }
        accepted_bytes = next_total;

        items.push(PlanItem {
            id: id.clone(),
            path: rec.path.clone(),
            size_gb: rec.size_gb,
            category: rec.category.clone(),
            cleanup_method: rec.cleanup_method.clone(),
        });
    }

    CleanupPlan {
        report_id: report_id.to_string(),
        items,
        rejected,
        total_size_gb: bytes_to_gb(accepted_bytes),
        recycle_budget_gb: bytes_to_gb(context.budget_bytes),
    }
}

fn bytes_to_gb(bytes: u64) -> f64 {
    (bytes as f64) / (1024.0 * 1024.0 * 1024.0)
}

pub(crate) fn build_plan(
    report: &ScanReport,
    report_id: &str,
    candidate_ids: &[String],
) -> Result<CleanupPlan, AppError> {
    let sample = PathBuf::from(&report.drive).join(".");
    let context = probe_recycle_context(&sample).map_err(AppError::Message)?;
    Ok(build_plan_with(report, report_id, candidate_ids, &context))
}

// ==== 回收站执行 ====

pub(crate) trait Recycler {
    fn recycle(&self, path: &Path) -> Result<(), String>;
}

/// 生产实现：SHFileOperationW FO_DELETE + FOF_ALLOWUNDO（回收站优先）。
pub(crate) struct ShellRecycler;

impl Recycler for ShellRecycler {
    fn recycle(&self, path: &Path) -> Result<(), String> {
        use windows_sys::Win32::UI::Shell::{
            SHFileOperationW, SHFILEOPSTRUCTW, FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION,
            FOF_NOCONFIRMMKDIR, FOF_NOERRORUI, FOF_SILENT,
        };

        // 路径必须是无尾随反斜杠的绝对路径：带尾随反斜杠的删除会绕过回收站直接永久删除。
        let mut text = path.to_string_lossy().to_string();
        while text.len() > 3 && text.ends_with('\\') {
            text.pop();
        }
        if !Path::new(&text).is_absolute() {
            return Err("仅支持绝对路径回收。".to_string());
        }
        // pFrom 要求双 NUL 结尾的 double-null-terminated 字符串。
        let mut from: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        from.push(0);

        let mut operation: SHFILEOPSTRUCTW = unsafe { std::mem::zeroed() };
        operation.wFunc = FO_DELETE;
        operation.pFrom = from.as_ptr();
        operation.fFlags = (FOF_ALLOWUNDO
            | FOF_NOCONFIRMATION
            | FOF_SILENT
            | FOF_NOERRORUI
            | FOF_NOCONFIRMMKDIR) as u16;

        let code = unsafe { SHFileOperationW(&mut operation) };
        if code != 0 {
            return Err(format!("回收站操作失败，错误码 {code}。"));
        }
        if operation.fAnyOperationsAborted != 0 {
            return Err("回收站操作被中止。".to_string());
        }
        if path.exists() {
            return Err("回收后目标仍存在于原位置。".to_string());
        }
        Ok(())
    }
}

/// 执行清理：与 plan 共用 build_plan_with，之后逐项做删除时刻复判再回收。
pub(crate) fn execute_with<R: Recycler>(
    report: &ScanReport,
    report_id: &str,
    candidate_ids: &[String],
    context: &RecycleContext,
    log_root: &Path,
    recycler: &R,
) -> Result<CleanupOutcome, AppError> {
    let plan = build_plan_with(report, report_id, candidate_ids, context);
    let rejected = plan.rejected.clone();
    let mut results = Vec::new();
    let mut recycled_count = 0u32;
    let mut failed_count = 0u32;
    let mut recycled_bytes = 0u64;

    for item in &plan.items {
        let path = PathBuf::from(&item.path);
        // 删除时刻复判：blocklist 复判 + 路径三重校验（build 与执行之间的窗口由这一步兜住）。
        let gate = reclassify_gate(&item.path).and_then(|()| validate_path_identity(&path).map(|_| ()));

        let (status, message) = match gate {
            Ok(()) => match recycler.recycle(&path) {
                Ok(()) => {
                    recycled_count += 1;
                    recycled_bytes +=
                        (item.size_gb.max(0.0) * 1024.0 * 1024.0 * 1024.0).ceil() as u64;
                    ("recycled".to_string(), None)
                }
                Err(error) => {
                    failed_count += 1;
                    ("failed".to_string(), Some(error))
                }
            },
            Err(reason) => {
                failed_count += 1;
                ("failed".to_string(), Some(reason))
            }
        };

        let entry = ActionLogEntry {
            ts: now_iso8601(),
            report_id: report_id.to_string(),
            id: item.id.clone(),
            path: item.path.clone(),
            size_gb: item.size_gb,
            action: "recycle".to_string(),
            // 审计日志用 ok/failed；UI outcome 用 recycled/failed。
            status: if status == "recycled" { "ok" } else { "failed" }.to_string(),
            message: message.clone(),
        };
        let _ = append_action_log(log_root, &entry);

        results.push(CleanupItemOutcome {
            id: item.id.clone(),
            path: item.path.clone(),
            size_gb: item.size_gb,
            status,
            message,
        });
    }

    Ok(CleanupOutcome {
        report_id: report_id.to_string(),
        results,
        rejected,
        recycled_count,
        failed_count,
        total_size_gb: bytes_to_gb(recycled_bytes),
        action_log_path: action_log_path(log_root)
            .to_string_lossy()
            .to_string(),
    })
}

pub(crate) fn execute(
    report: &ScanReport,
    report_id: &str,
    candidate_ids: &[String],
    log_root: &Path,
) -> Result<CleanupOutcome, AppError> {
    let sample = PathBuf::from(&report.drive).join(".");
    let context = probe_recycle_context(&sample).map_err(AppError::Message)?;
    execute_with(report, report_id, candidate_ids, &context, log_root, &ShellRecycler)
}

// ==== action-log 审计 ====

fn action_log_path(root: &Path) -> PathBuf {
    root.join("action-log.jsonl")
}

fn append_action_log(root: &Path, entry: &ActionLogEntry) -> Result<(), AppError> {
    fs::create_dir_all(root)?;
    let line = serde_json::to_string(entry)?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(action_log_path(root))?;
    writeln!(file, "{line}")?;
    Ok(())
}

pub(crate) fn read_action_log(root: &Path, limit: usize) -> Vec<ActionLogEntry> {
    let path = action_log_path(root);
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let mut entries: Vec<ActionLogEntry> = BufReader::new(file)
        .lines()
        .flatten()
        .filter_map(|line| serde_json::from_str::<ActionLogEntry>(&line).ok())
        .collect();
    // 最新在前，按上限截断。
    entries.reverse();
    entries.truncate(limit);
    entries
}

fn now_iso8601() -> String {
    chrono::Utc::now().to_rfc3339()
}

pub(crate) fn action_log_read_limit() -> usize {
    ACTION_LOG_READ_LIMIT
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::classify::Recommendation;
    use crate::report::test_scan_report;

    fn context(budget_bytes: f64) -> RecycleContext {
        RecycleContext {
            budget_bytes: budget_bytes as u64,
        }
    }

    const GB: f64 = 1024.0 * 1024.0 * 1024.0;

    fn recommendation(
        id: &str,
        path: &str,
        category: &str,
        risk: &str,
        cleanable: bool,
        size_gb: f64,
    ) -> Recommendation {
        Recommendation {
            id: id.to_string(),
            path: path.to_string(),
            size_gb,
            category: category.to_string(),
            risk: risk.to_string(),
            confidence: 0.8,
            reason: "test".to_string(),
            manual_steps: Vec::new(),
            cleanable,
            blocked_reason: None,
            cleanup_method: Some("recycle".to_string()),
            requires_app_closed: false,
            source: "scanner".to_string(),
        }
    }

    fn report_with(recommendations: Vec<Recommendation>) -> ScanReport {
        let mut report = test_scan_report("m.md", "j.json");
        report.recommendations = recommendations;
        report
    }

    fn temp_dir_unique(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "wcdca-cleanup-test-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 计数器式 FakeRecycler：记录调用，可注入失败。
    struct FakeRecycler {
        fail: bool,
        calls: std::sync::Mutex<Vec<PathBuf>>,
    }

    impl FakeRecycler {
        fn new(fail: bool) -> Self {
            Self {
                fail,
                calls: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    impl Recycler for FakeRecycler {
        fn recycle(&self, path: &Path) -> Result<(), String> {
            if self.fail {
                return Err("注入的回收失败".to_string());
            }
            self.calls.lock().unwrap().push(path.to_path_buf());
            // 模拟删除成功：移除目标。
            let _ = fs::remove_dir_all(path);
            let _ = fs::remove_file(path);
            Ok(())
        }
    }

    #[test]
    fn plan_accepts_existing_low_risk_cache_dir() {
        let root = temp_dir_unique("accept");
        let cache = root.join("cache");
        fs::create_dir_all(&cache).unwrap();
        let report = report_with(vec![recommendation(
            "id-cache",
            cache.to_str().unwrap(),
            "low-risk-cache",
            "low",
            true,
            1.0,
        )]);
        let plan = build_plan_with(&report, "scan-1", &["id-cache".into()], &context(100.0 * GB));
        assert_eq!(plan.items.len(), 1, "rejected: {:?}", plan.rejected);
        assert!(plan.rejected.is_empty());
        assert_eq!(plan.report_id, "scan-1");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn plan_rejects_non_cache_category() {
        let root = temp_dir_unique("user-data");
        let report = report_with(vec![recommendation(
            "id-ud",
            root.to_str().unwrap(),
            "user-data",
            "medium",
            true,
            1.0,
        )]);
        let plan = build_plan_with(&report, "scan-1", &["id-ud".into()], &context(100.0 * GB));
        assert!(plan.items.is_empty());
        assert_eq!(plan.rejected.len(), 1);
        assert!(plan.rejected[0].reason.contains("low-risk-cache"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn plan_rejects_not_cleanable_and_unknown_id() {
        let root = temp_dir_unique("gates");
        let report = report_with(vec![recommendation(
            "id-blocked",
            root.to_str().unwrap(),
            "low-risk-cache",
            "low",
            false,
            1.0,
        )]);
        let plan = build_plan_with(
            &report,
            "scan-1",
            &["id-blocked".into(), "missing-id".into()],
            &context(100.0 * GB),
        );
        assert!(plan.items.is_empty());
        assert_eq!(plan.rejected.len(), 2);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn plan_recheck_rejects_system_managed_path() {
        // 报告字段伪造成 low-risk-cache，但路径复判为系统托管：删除时刻复判必须抓住。
        let report = report_with(vec![recommendation(
            "id-winsxs",
            "C:\\Windows\\WinSxS\\Download",
            "low-risk-cache",
            "low",
            true,
            3.0,
        )]);
        let plan = build_plan_with(&report, "scan-1", &["id-winsxs".into()], &context(100.0 * GB));
        assert!(plan.items.is_empty());
        assert_eq!(plan.rejected.len(), 1);
        assert!(
            plan.rejected[0].reason.contains("复判"),
            "应报告复判不通过：{}",
            plan.rejected[0].reason
        );
    }

    #[test]
    fn plan_rejects_missing_path() {
        let report = report_with(vec![recommendation(
            "id-ghost",
            std::env::temp_dir()
                .join("wcdca-definitely-missing-path")
                .to_str()
                .unwrap(),
            "low-risk-cache",
            "low",
            true,
            1.0,
        )]);
        let plan = build_plan_with(&report, "scan-1", &["id-ghost".into()], &context(100.0 * GB));
        assert!(plan.items.is_empty());
        assert_eq!(plan.rejected.len(), 1);
    }

    #[test]
    fn plan_rejects_reparse_junction() {
        let root = temp_dir_unique("junction");
        let target = root.join("real-cache");
        // 段名必须命中 low-risk-cache 规则（cache 前缀），否则会在复判层先行拒绝、测不到 reparse 层。
        let link = root.join("cache-link");
        fs::create_dir_all(&target).unwrap();
        let status = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .status()
            .unwrap();
        assert!(status.success(), "mklink /J failed");

        let report = report_with(vec![recommendation(
            "id-link",
            link.to_str().unwrap(),
            "low-risk-cache",
            "low",
            true,
            1.0,
        )]);
        let plan = build_plan_with(&report, "scan-1", &["id-link".into()], &context(100.0 * GB));
        assert!(plan.items.is_empty(), "junction 目标必须被拒绝");
        assert_eq!(plan.rejected.len(), 1);
        assert!(
            plan.rejected[0].reason.contains("重解析")
                || plan.rejected[0].reason.contains("符号链接"),
            "原因应为重解析/链接拒绝：{}",
            plan.rejected[0].reason
        );

        // 测试区允许清理自建临时目录（先删 junction 本体，不动目标内容）。
        fs::remove_dir(&link).unwrap();
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn plan_rejects_over_budget() {
        let root = temp_dir_unique("budget");
        let cache = root.join("cache");
        fs::create_dir_all(&cache).unwrap();
        let report = report_with(vec![recommendation(
            "id-big",
            cache.to_str().unwrap(),
            "low-risk-cache",
            "low",
            true,
            5.0,
        )]);
        // 预算 1GB < 候选 5GB。
        let plan = build_plan_with(&report, "scan-1", &["id-big".into()], &context(1.0 * GB));
        assert!(plan.items.is_empty());
        assert_eq!(plan.rejected.len(), 1);
        assert!(plan.rejected[0].reason.contains("预算"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn plan_deduplicates_repeated_ids() {
        let root = temp_dir_unique("dedup");
        let cache = root.join("cache");
        fs::create_dir_all(&cache).unwrap();
        let report = report_with(vec![recommendation(
            "id-one",
            cache.to_str().unwrap(),
            "low-risk-cache",
            "low",
            true,
            1.0,
        )]);
        let plan = build_plan_with(
            &report,
            "scan-1",
            &["id-one".into(), "id-one".into(), " id-one ".into()],
            &context(100.0 * GB),
        );
        assert_eq!(plan.items.len(), 1, "rejected: {:?}", plan.rejected);
        assert!(plan.rejected.is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn execute_with_fake_recycler_records_outcome_and_log() {
        let root = temp_dir_unique("exec-ok");
        let cache = root.join("cache");
        fs::create_dir_all(&cache).unwrap();
        let report = report_with(vec![recommendation(
            "id-cache",
            cache.to_str().unwrap(),
            "low-risk-cache",
            "low",
            true,
            1.0,
        )]);
        let log_root = root.join("reports");
        let recycler = FakeRecycler::new(false);
        let outcome = execute_with(
            &report,
            "scan-1",
            &["id-cache".into()],
            &context(100.0 * GB),
            &log_root,
            &recycler,
        )
        .unwrap();

        assert_eq!(outcome.recycled_count, 1, "outcome: {outcome:?}");
        assert_eq!(outcome.failed_count, 0);
        assert_eq!(outcome.results[0].status, "recycled");
        assert!(!cache.exists(), "FakeRecycler 应已移除目标");
        assert_eq!(recycler.calls.lock().unwrap().len(), 1);

        let log = read_action_log(&log_root, 10);
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].status, "ok");
        assert_eq!(log[0].action, "recycle");
        assert_eq!(log[0].report_id, "scan-1");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn execute_with_failing_recycler_marks_failed_and_logs() {
        let root = temp_dir_unique("exec-fail");
        let cache = root.join("cache");
        fs::create_dir_all(&cache).unwrap();
        let report = report_with(vec![recommendation(
            "id-cache",
            cache.to_str().unwrap(),
            "low-risk-cache",
            "low",
            true,
            1.0,
        )]);
        let log_root = root.join("reports");
        let recycler = FakeRecycler::new(true);
        let outcome = execute_with(
            &report,
            "scan-1",
            &["id-cache".into()],
            &context(100.0 * GB),
            &log_root,
            &recycler,
        )
        .unwrap();

        assert_eq!(outcome.recycled_count, 0);
        assert_eq!(outcome.failed_count, 1);
        assert_eq!(outcome.results[0].status, "failed");
        assert!(cache.exists(), "失败时目标必须保留");
        let log = read_action_log(&log_root, 10);
        assert_eq!(log[0].status, "failed");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn execute_recheck_blocks_replaced_path_before_recycle() {
        // 删除时刻复判：报告记录 low-risk-cache，但执行前路径语义已变（用 system-managed 路径替换报告项）。
        let report = report_with(vec![recommendation(
            "id-swap",
            "C:\\Windows\\System32\\drivers",
            "low-risk-cache",
            "low",
            true,
            1.0,
        )]);
        let root = temp_dir_unique("recheck");
        let log_root = root.join("reports");
        let recycler = FakeRecycler::new(false);
        let outcome = execute_with(
            &report,
            "scan-1",
            &["id-swap".into()],
            &context(100.0 * GB),
            &log_root,
            &recycler,
        )
        .unwrap();
        // 复判在 build_plan 层即拒绝：不进 results（failed_count），进 rejected；回收器绝不触达。
        assert_eq!(outcome.recycled_count, 0);
        assert_eq!(outcome.failed_count, 0);
        assert_eq!(outcome.rejected.len(), 1);
        assert!(recycler.calls.lock().unwrap().is_empty(), "不得触达回收器");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn reclassify_gate_blocks_system_managed_and_non_cache() {
        // 删除时刻复判的直接覆盖（execute 循环里与 build_plan 共用同一函数）。
        assert!(reclassify_gate("C:\\Windows\\WinSxS").is_err());
        assert!(reclassify_gate("C:\\Users\\me\\Documents").is_err());
        assert!(reclassify_gate("C:\\Users\\me\\AppData\\Local\\npm-cache").is_ok());
    }

    #[test]
    fn action_log_reads_latest_first_with_limit_and_skips_broken_lines() {
        let root = temp_dir_unique("log");
        let log_root = root.join("reports");
        for index in 0..3 {
            let entry = ActionLogEntry {
                ts: format!("2026-09-26T00:00:0{index}Z"),
                report_id: "scan-1".into(),
                id: format!("id-{index}"),
                path: format!("C:\\tmp\\{index}"),
                size_gb: 1.0,
                action: "recycle".into(),
                status: "ok".into(),
                message: None,
            };
            append_action_log(&log_root, &entry).unwrap();
        }
        // 追加坏行：读取必须跳过而不是失败。
        fs::OpenOptions::new()
            .append(true)
            .open(action_log_path(&log_root))
            .unwrap()
            .write_all(b"{broken json\n")
            .unwrap();

        let log = read_action_log(&log_root, 2);
        assert_eq!(log.len(), 2);
        assert_eq!(log[0].id, "id-2", "最新在前");
        assert_eq!(log[1].id, "id-1");
        let _ = fs::remove_dir_all(&root);
    }

    /// 真实回收站冒烟：默认跳过，设 `WCDCA_RECYCLE_SMOKE=1` 时执行。
    /// 该用例会把一个自建 1KB 临时文件送入当前用户的回收站（可自行清空）。
    #[test]
    fn recycle_smoke_moves_file_to_recycle_bin() {
        if std::env::var("WCDCA_RECYCLE_SMOKE").is_err() {
            eprintln!("skip recycle smoke (set WCDCA_RECYCLE_SMOKE=1 to run)");
            return;
        }
        let root = temp_dir_unique("smoke");
        let file = root.join("smoke-file.bin");
        fs::write(&file, [0u8; 1024]).unwrap();
        ShellRecycler.recycle(&file).expect("回收站操作失败");
        assert!(!file.exists(), "文件应已离开原位置（进入回收站）");
        let _ = fs::remove_dir_all(&root);
    }
}
