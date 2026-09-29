//! Windows C 盘清理顾问 — 只读扫描/报告应用的 Rust 边界。
//!
//! 模块划分（Phase 1 结构拆分，行为与 IPC 契约不变；Phase 5 增补 `cleanup`）：
//! - `commands`：Tauri IPC 命令与扫描编排（`run_scan_worker`）。
//! - `tasks`：内存任务表与状态机（终态守卫、取消、PID 登记、环形淘汰）。
//! - `kernel`：原生扫描内核（Phase 2 默认内核，进程内 Rust 遍历，公开给 wcdca-scan 测试二进制）。
//! - `scanner`：PowerShell 兜底内核进程侧（脚本解析、启动参数、stdout 解码、看门狗、进度协议）。
//! - `classify`：推荐分类规则与缓存目录聚合。
//! - `report`：报告组装、报告目录/索引管理、环形淘汰。
//! - `cleanup`：Phase 5 写权限边界（仅 plan/execute 低风险缓存回收站清理，全项目唯一删除面）。
//! - `errors`：IPC 错误类型。

mod classify;
mod cleanup;
mod commands;
mod errors;
pub mod kernel;
mod report;
mod scanner;
mod tasks;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

pub fn run() {
    tauri::Builder::default()
        .manage(commands::AppState {
            tasks: Arc::new(Mutex::new(HashMap::new())),
        })
        .invoke_handler(tauri::generate_handler![
            commands::start_scan,
            commands::get_scan_status,
            commands::cancel_scan,
            commands::get_scan_report,
            commands::load_latest_report,
            commands::list_reports,
            commands::load_report_by_id,
            commands::reveal_report,
            commands::plan_cleanup,
            commands::execute_cleanup,
            commands::get_action_log
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}
