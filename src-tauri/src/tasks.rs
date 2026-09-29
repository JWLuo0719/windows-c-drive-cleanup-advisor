use crate::errors::AppError;
use crate::report::ScanReport;
use chrono::Utc;
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

pub(crate) type TaskMap = Arc<Mutex<HashMap<String, ScanTask>>>;

/// 内存中保留的历史扫描任务上限；超出时从最旧的终态任务开始淘汰。
const MAX_TASK_HISTORY: usize = 20;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScanStatus {
    pub(crate) scan_id: String,
    pub(crate) phase: String,
    pub(crate) percent: u8,
    pub(crate) message: String,
    pub(crate) started_at: Option<String>,
    pub(crate) completed_at: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) markdown_report_path: Option<String>,
    pub(crate) json_report_path: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ScanTask {
    pub(crate) status: ScanStatus,
    pub(crate) report: Option<ScanReport>,
    pub(crate) process_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ScanProgressEvent {
    pub(crate) scan_id: String,
    pub(crate) phase: String,
    pub(crate) percent: u8,
    pub(crate) message: String,
}

pub(crate) fn update_task_status(
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
    // 终态守卫：completed/failed/cancelled 一旦写入即冻结。
    // 迟到的进度或失败更新不得把 cancelled 覆盖回 running（取消被静默撤销的根因）。
    if is_terminal_phase(&task.status.phase) {
        return None;
    }
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

pub(crate) fn is_terminal_phase(phase: &str) -> bool {
    matches!(phase, "completed" | "failed" | "cancelled")
}

pub(crate) fn insert_queued_scan_task(
    tasks: &TaskMap,
    scan_id: &str,
    status: ScanStatus,
) -> Result<(), AppError> {
    let mut locked = tasks
        .lock()
        .map_err(|_| AppError::Message("扫描任务状态暂不可用。".to_string()))?;

    if locked
        .values()
        .any(|task| !is_terminal_phase(&task.status.phase))
    {
        return Err(AppError::Message(
            "已有扫描正在运行，请先等待完成或取消当前扫描。".to_string(),
        ));
    }

    locked.insert(
        scan_id.to_string(),
        ScanTask {
            status,
            report: None,
            process_id: None,
        },
    );
    prune_task_history(&mut locked);
    Ok(())
}

/// 任务表环形淘汰：仅淘汰终态任务，从最旧的开始，直到总数不超过 `MAX_TASK_HISTORY`。
pub(crate) fn prune_task_history(locked: &mut HashMap<String, ScanTask>) {
    if locked.len() <= MAX_TASK_HISTORY {
        return;
    }
    let mut terminal: Vec<(String, String)> = locked
        .iter()
        .filter(|(_, task)| is_terminal_phase(&task.status.phase))
        .map(|(id, task)| {
            let key = task
                .status
                .completed_at
                .clone()
                .or_else(|| task.status.started_at.clone())
                .unwrap_or_default();
            (key, id.clone())
        })
        .collect();
    // RFC3339 字符串按字典序即时间序；同刻以 scan_id 保证确定性。
    terminal.sort();
    let excess = locked.len() - MAX_TASK_HISTORY;
    for (_, id) in terminal.into_iter().take(excess) {
        locked.remove(&id);
    }
}

pub(crate) fn cancel_scan_task(
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

/// 原子登记扫描器 PID：任务已进入终态（如登记窗口内被取消）则拒绝登记。
/// 返回 false 时调用方必须自行清理刚启动的子进程，避免孤儿扫描。
pub(crate) fn try_register_process(tasks: &TaskMap, scan_id: &str, pid: u32) -> bool {
    let Ok(mut locked) = tasks.lock() else {
        return false;
    };
    let Some(task) = locked.get_mut(scan_id) else {
        return false;
    };
    if is_terminal_phase(&task.status.phase) {
        return false;
    }
    task.process_id = Some(pid);
    true
}

pub(crate) fn complete_scan_task(
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

    // 终态守卫：报告组装期间用户取消则保持 cancelled，报告文件仍留在磁盘供“载入最近报告”取用。
    if is_terminal_phase(&task.status.phase) {
        return Ok(None);
    }

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

pub(crate) fn current_phase(tasks: &TaskMap, scan_id: &str) -> Option<String> {
    let locked = tasks.lock().ok()?;
    locked.get(scan_id).map(|task| task.status.phase.clone())
}

#[cfg(test)]
pub(crate) fn test_scan_status(scan_id: &str, phase: &str) -> ScanStatus {
    ScanStatus {
        scan_id: scan_id.to_string(),
        phase: phase.to_string(),
        percent: if is_terminal_phase(phase) { 100 } else { 25 },
        message: "test status".to_string(),
        started_at: Some("2026-06-21T00:00:00Z".to_string()),
        completed_at: is_terminal_phase(phase).then(|| "2026-06-21T00:01:00Z".to_string()),
        error: None,
        markdown_report_path: None,
        json_report_path: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::test_scan_report;

    #[test]
    fn insert_queued_scan_task_rejects_existing_active_scan() {
        let tasks: TaskMap = Arc::new(Mutex::new(HashMap::new()));
        {
            let mut locked = tasks.lock().unwrap();
            locked.insert(
                "active-scan".to_string(),
                ScanTask {
                    status: test_scan_status("active-scan", "running"),
                    report: None,
                    process_id: Some(456),
                },
            );
        }

        let error =
            insert_queued_scan_task(&tasks, "new-scan", test_scan_status("new-scan", "queued"))
                .unwrap_err()
                .to_string();

        assert!(error.contains("已有扫描正在运行"));
        let locked = tasks.lock().unwrap();
        assert!(!locked.contains_key("new-scan"));
    }

    #[test]
    fn insert_queued_scan_task_allows_new_scan_after_terminal_history() {
        let tasks: TaskMap = Arc::new(Mutex::new(HashMap::new()));
        {
            let mut locked = tasks.lock().unwrap();
            locked.insert(
                "old-scan".to_string(),
                ScanTask {
                    status: test_scan_status("old-scan", "completed"),
                    report: None,
                    process_id: None,
                },
            );
        }

        insert_queued_scan_task(&tasks, "new-scan", test_scan_status("new-scan", "queued"))
            .unwrap();

        let locked = tasks.lock().unwrap();
        assert!(locked.contains_key("old-scan"));
        assert_eq!(locked.get("new-scan").unwrap().status.phase, "queued");
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
            privacy: crate::report::PrivacyLedger { uploaded: false },
            recommendations: Vec::new(),
            markdown_report_path: "C:\\reports\\scan.md".to_string(),
            json_report_path: "C:\\reports\\scan.json".to_string(),
            top_rows: Vec::new(),
            drilldowns: Vec::new(),
            large_files: Vec::new(),
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
    fn update_task_status_refuses_write_back_after_terminal() {
        let scan_id = "scan-guard";
        let tasks: TaskMap = Arc::new(Mutex::new(HashMap::new()));
        {
            let mut locked = tasks.lock().unwrap();
            locked.insert(
                scan_id.to_string(),
                ScanTask {
                    status: test_scan_status(scan_id, "cancelled"),
                    report: None,
                    process_id: None,
                },
            );
        }

        // 迟到的 running 进度与 failed 错误都不得覆盖终态。
        assert!(update_task_status(
            &tasks,
            scan_id,
            "running",
            42,
            "late progress",
            None,
            None,
            None
        )
        .is_none());
        assert!(update_task_status(
            &tasks,
            scan_id,
            "failed",
            100,
            "late failure",
            Some("boom".to_string()),
            None,
            None
        )
        .is_none());

        let locked = tasks.lock().unwrap();
        let task = locked.get(scan_id).unwrap();
        assert_eq!(task.status.phase, "cancelled");
        assert_ne!(task.status.message, "late progress");
    }

    #[test]
    fn queued_window_cancel_prevents_running_transition() {
        // 取消全链路（queued 窗口）：排队后立即取消 → worker 的 running 迁移必须被拒绝。
        let scan_id = "scan-queued-cancel";
        let tasks: TaskMap = Arc::new(Mutex::new(HashMap::new()));
        insert_queued_scan_task(&tasks, scan_id, test_scan_status(scan_id, "queued")).unwrap();

        let (process_id, cancelled, should_emit) = cancel_scan_task(&tasks, scan_id).unwrap();
        assert_eq!(process_id, None);
        assert!(should_emit);
        assert_eq!(cancelled.phase, "cancelled");

        // worker 醒来后不得把 cancelled 翻回 running，也不得登记 PID。
        assert!(
            update_task_status(&tasks, scan_id, "running", 8, "start", None, None, None).is_none()
        );
        assert!(!try_register_process(&tasks, scan_id, 4242));

        let locked = tasks.lock().unwrap();
        assert_eq!(locked.get(scan_id).unwrap().status.phase, "cancelled");
        assert!(locked.get(scan_id).unwrap().process_id.is_none());
    }

    #[test]
    fn late_progress_does_not_resurrect_cancelled_scan() {
        // 取消全链路（running 窗口）：取消后迟到的进度事件保持终态不变。
        let scan_id = "scan-late-progress";
        let tasks: TaskMap = Arc::new(Mutex::new(HashMap::new()));
        insert_queued_scan_task(&tasks, scan_id, test_scan_status(scan_id, "queued")).unwrap();
        assert!(try_register_process(&tasks, scan_id, 456));
        cancel_scan_task(&tasks, scan_id).unwrap();

        assert!(
            update_task_status(&tasks, scan_id, "running", 88, "late", None, None, None).is_none()
        );
        assert!(!try_register_process(&tasks, scan_id, 789));

        let locked = tasks.lock().unwrap();
        assert_eq!(locked.get(scan_id).unwrap().status.phase, "cancelled");
    }

    #[test]
    fn try_register_process_rejects_terminal_task() {
        let scan_id = "scan-register";
        let tasks: TaskMap = Arc::new(Mutex::new(HashMap::new()));
        {
            let mut locked = tasks.lock().unwrap();
            locked.insert(
                scan_id.to_string(),
                ScanTask {
                    status: test_scan_status(scan_id, "completed"),
                    report: None,
                    process_id: None,
                },
            );
        }
        assert!(!try_register_process(&tasks, scan_id, 111));
        let locked = tasks.lock().unwrap();
        assert!(locked.get(scan_id).unwrap().process_id.is_none());
    }

    #[test]
    fn complete_scan_task_refuses_when_cancelled_won_race() {
        // 报告组装期间用户取消：complete 不得覆盖 cancelled，报告仍可从磁盘载入。
        let scan_id = "scan-complete-race";
        let tasks: TaskMap = Arc::new(Mutex::new(HashMap::new()));
        insert_queued_scan_task(&tasks, scan_id, test_scan_status(scan_id, "queued")).unwrap();
        update_task_status(&tasks, scan_id, "running", 76, "building", None, None, None).unwrap();
        cancel_scan_task(&tasks, scan_id).unwrap();

        let report = test_scan_report("C:\\reports\\scan.md", "C:\\reports\\scan.json");
        assert!(complete_scan_task(&tasks, scan_id, report).unwrap().is_none());

        let locked = tasks.lock().unwrap();
        let task = locked.get(scan_id).unwrap();
        assert_eq!(task.status.phase, "cancelled");
        assert!(task.report.is_none());
    }

    #[test]
    fn prune_task_history_keeps_most_recent_terminal_tasks() {
        let mut locked = HashMap::new();
        for index in 0..25 {
            let scan_id = format!("scan-{index:02}");
            let mut status = test_scan_status(&scan_id, "completed");
            status.completed_at = Some(format!("2026-09-25T00:{index:02}:00Z"));
            locked.insert(
                scan_id,
                ScanTask {
                    status,
                    report: None,
                    process_id: None,
                },
            );
        }
        let mut status = test_scan_status("scan-active", "running");
        status.completed_at = None;
        locked.insert(
            "scan-active".to_string(),
            ScanTask {
                status,
                report: None,
                process_id: None,
            },
        );

        prune_task_history(&mut locked);

        assert_eq!(locked.len(), MAX_TASK_HISTORY);
        // 26 条超出上限 6 条：最旧的 6 个终态任务（scan-00..scan-05）被淘汰，其余与活动任务保留。
        assert!(!locked.contains_key("scan-00"));
        assert!(!locked.contains_key("scan-05"));
        assert!(locked.contains_key("scan-06"));
        assert!(locked.contains_key("scan-24"));
        assert!(locked.contains_key("scan-active"));
    }
}
