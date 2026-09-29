use crate::errors::AppError;
use std::{
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Manager};

/// 扫描器 stdout 在该时长内没有任何输出即判定卡死，强制终止进程树。
/// 取值依据：烟测实测最慢的合法停顿（单目录深挖）约 5 分钟，取 6 倍余量。
pub(crate) const SCAN_STALL_TIMEOUT: Duration = Duration::from_secs(30 * 60);
/// 看门狗轮询 `try_wait` 的间隔。
pub(crate) const WATCHDOG_POLL_INTERVAL: Duration = Duration::from_millis(500);

// ==== 进度协议常量表 ====
// 标记行格式：`[WCDCA_PROGRESS] {percent}|{code}`。
// 三方对齐（一致性由 scripts/Test-ScannerContract.ps1 断言，改动必须同步三处）：
// - scripts/Scan-CDriveCleanupAdvisor.ps1 的 `$ProgressPercent` / `$ProgressCode`
// - 本文件 `PROGRESS_PCT_*` / `PROGRESS_CODE_*`（parse_scanner_progress 的唯一消息来源）
// - src/App.tsx 的 `PROGRESS_PCT`（阶段文案分段边界）

/// 标记行前缀。
pub(crate) const PROGRESS_MARKER_PREFIX: &str = "[WCDCA_PROGRESS] ";
/// 百分比与阶段码之间的分隔符。
pub(crate) const PROGRESS_MARKER_SEPARATOR: char = '|';

// 固定百分比阶段码：发射时百分比即协议的一部分，不得漂移。
pub(crate) const PROGRESS_CODE_DRIVE_INFO: &str = "DRIVE_INFO";
pub(crate) const PROGRESS_PCT_DRIVE_INFO: u8 = 12;
pub(crate) const PROGRESS_CODE_TOP_ROOTS: &str = "TOP_ROOTS";
pub(crate) const PROGRESS_PCT_TOP_ROOTS: u8 = 18;
pub(crate) const PROGRESS_CODE_LARGE_FILES: &str = "LARGE_FILES";
pub(crate) const PROGRESS_PCT_LARGE_FILES: u8 = 65;
pub(crate) const PROGRESS_CODE_SYSTEM_INFO: &str = "SYSTEM_INFO";
pub(crate) const PROGRESS_PCT_SYSTEM_INFO: u8 = 74;
pub(crate) const PROGRESS_CODE_DISM: &str = "DISM";
pub(crate) const PROGRESS_PCT_DISM: u8 = 82;
pub(crate) const PROGRESS_CODE_REPORT: &str = "REPORT";
pub(crate) const PROGRESS_PCT_REPORT: u8 = 88;
pub(crate) const PROGRESS_CODE_JSON: &str = "JSON";
pub(crate) const PROGRESS_PCT_JSON: u8 = 94;

// 心跳码：码后以冒号接当前路径（动态百分比，落在所属阶段区间内）。
pub(crate) const PROGRESS_CODE_TOP_ROOTS_SCAN: &str = "TOP_ROOTS_SCAN:";
pub(crate) const PROGRESS_CODE_LARGE_FILES_SCAN: &str = "LARGE_FILES_SCAN:";
pub(crate) const PROGRESS_CODE_DRILLDOWN_SCAN: &str = "DRILLDOWN_SCAN:";
pub(crate) const PROGRESS_CODE_DRILLDOWN: &str = "DRILLDOWN:";

/// deep 模式重点目录深挖区间起点（DRILLDOWN 首点），
/// 也是 App.tsx 切换“深挖重点目录”阶段文案的边界。
pub(crate) const PROGRESS_PCT_DRILLDOWN_START: u8 = 35;

pub(crate) fn normalize_drive(input: &str) -> Result<String, AppError> {
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

pub(crate) fn resolve_scanner_script(app: &AppHandle) -> Result<PathBuf, AppError> {
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

pub(crate) fn resource_scanner_candidates(resource_dir: &Path) -> Vec<PathBuf> {
    let scanner_file = Path::new("Scan-CDriveCleanupAdvisor.ps1");
    let mut candidates = vec![
        resource_dir.join("scripts").join(scanner_file),
        resource_dir.join(scanner_file),
        resource_dir.join("_up_").join("scripts").join(scanner_file),
        resource_dir.join("_up_").join(scanner_file),
    ];

    if let Some(parent) = resource_dir.parent() {
        candidates.push(parent.join("_up_").join("scripts").join(scanner_file));
        candidates.push(parent.join("_up_").join(scanner_file));
    }

    candidates
}

pub(crate) fn scanner_script_args(
    script_path: &Path,
    drive: &str,
    output_dir: &Path,
    top_count: u32,
    large_file_mb: u32,
    include_common_roots: bool,
) -> Vec<String> {
    let mut args = vec![
        script_path.display().to_string(),
        "-Drive".to_string(),
        drive.to_string(),
        "-OutputDir".to_string(),
        output_dir.display().to_string(),
        "-TopCount".to_string(),
        top_count.to_string(),
        "-LargeFileMB".to_string(),
        large_file_mb.to_string(),
        "-IncludeJson".to_string(),
    ];

    if !include_common_roots {
        args.push("-SkipCommonRoots".to_string());
    }

    args
}

pub(crate) fn resolve_powershell() -> String {
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

/// 将扫描器 stdout 的一行原始字节解码为文本。
/// PowerShell 5.1 管道输出使用系统 OEM 码页（简体中文系统为 GBK/CP936），
/// 含中文路径的进度行不是合法 UTF-8。优先按 UTF-8 解码
///（PowerShell 7 或已切换编码的输出），失败后按 GBK 兜底，
/// 仍失败才做有损替换——保证任何字节序列都产出一行文本、读取永不中断。
pub(crate) fn decode_scanner_line(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => {
            let (text, _, had_errors) = encoding_rs::GBK.decode(bytes);
            if had_errors {
                String::from_utf8_lossy(bytes).into_owned()
            } else {
                text.into_owned()
            }
        }
    }
}

/// 逐行读取扫描器 stdout 并回调每行文本。
/// 回调返回 false 表示停止读取（例如任务已取消）；单行解码失败不中断后续行。
pub(crate) fn for_each_stdout_line<R: Read>(reader: R, mut on_line: impl FnMut(&str) -> bool) {
    let mut reader = BufReader::new(reader);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                while matches!(buf.last(), Some(b'\n') | Some(b'\r')) {
                    buf.pop();
                }
                if !on_line(&decode_scanner_line(&buf)) {
                    break;
                }
            }
        }
    }
}

/// 轮询等待子进程退出；超过 `stall_timeout` 没有任何输出活动即判定卡死，
/// 强制终止整个进程树后回收子进程，避免状态永远停留在 running。
pub(crate) fn wait_child_with_watchdog(
    child: &mut Child,
    last_activity: &Arc<Mutex<Instant>>,
    stall_timeout: Duration,
    poll_interval: Duration,
) -> Result<(std::process::ExitStatus, bool), AppError> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok((status, false));
        }
        let stalled = last_activity
            .lock()
            .map(|stamp| stamp.elapsed() >= stall_timeout)
            .unwrap_or(false);
        if stalled {
            let _ = Command::new("taskkill")
                .args(["/PID", &child.id().to_string(), "/T", "/F"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            let status = child.wait()?;
            return Ok((status, true));
        }
        thread::sleep(poll_interval);
    }
}

/// 解析一行 `[WCDCA_PROGRESS] {percent}|{code}` 标记。
/// 阶段码与消息文案的对应关系是进度协议的一部分：UI 只消费本函数产出的消息，
/// 不得反向解析消息文案推断阶段（见 PROGRESS_* 常量表注释）。
pub(crate) fn parse_scanner_progress(line: &str) -> Option<(u8, String)> {
    let payload = line.strip_prefix(PROGRESS_MARKER_PREFIX)?;
    let (percent_text, code) = payload.split_once(PROGRESS_MARKER_SEPARATOR)?;
    let percent = percent_text.trim().parse::<u8>().ok()?.min(99);
    let message = match code.trim() {
        PROGRESS_CODE_DRIVE_INFO => "正在读取磁盘容量和权限信息。".to_string(),
        PROGRESS_CODE_TOP_ROOTS => "正在扫描 C 盘顶层真实目录。".to_string(),
        PROGRESS_CODE_LARGE_FILES => "正在查找大文件。".to_string(),
        PROGRESS_CODE_SYSTEM_INFO => "正在读取 pagefile、休眠和系统托管项信息。".to_string(),
        PROGRESS_CODE_DISM => "正在读取 DISM 组件存储分析。".to_string(),
        PROGRESS_CODE_REPORT => "正在生成 Markdown 报告。".to_string(),
        PROGRESS_CODE_JSON => "正在写入 JSON 数据。".to_string(),
        other if other.starts_with(PROGRESS_CODE_TOP_ROOTS_SCAN) => {
            let path = other.trim_start_matches(PROGRESS_CODE_TOP_ROOTS_SCAN);
            if path.is_empty() {
                "正在统计 C 盘顶层目录，扫描仍在推进。".to_string()
            } else {
                format!("正在统计目录体量：{path}")
            }
        }
        other if other.starts_with(PROGRESS_CODE_LARGE_FILES_SCAN) => {
            let path = other.trim_start_matches(PROGRESS_CODE_LARGE_FILES_SCAN);
            if path.is_empty() {
                "正在枚举大文件候选，扫描仍在推进。".to_string()
            } else {
                format!("正在枚举大文件候选：{path}")
            }
        }
        other if other.starts_with(PROGRESS_CODE_DRILLDOWN_SCAN) => {
            let path = other.trim_start_matches(PROGRESS_CODE_DRILLDOWN_SCAN);
            if path.is_empty() {
                "正在统计重点目录内部体量。".to_string()
            } else {
                format!("正在统计重点目录内部体量：{path}")
            }
        }
        other if other.starts_with(PROGRESS_CODE_DRILLDOWN) => {
            let path = other.trim_start_matches(PROGRESS_CODE_DRILLDOWN);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant as StdInstant;

    #[test]
    fn normalize_drive_accepts_single_letter_only() {
        assert_eq!(normalize_drive("c").unwrap(), "C");
        assert_eq!(normalize_drive("D:").unwrap(), "D");
        assert!(normalize_drive("CC").is_err());
        assert!(normalize_drive("").is_err());
    }

    #[test]
    fn quick_scan_args_skip_common_root_drilldowns() {
        let args = scanner_script_args(
            Path::new(r"C:\app\Scan-CDriveCleanupAdvisor.ps1"),
            "C",
            Path::new(r"C:\reports\scan"),
            30,
            200,
            false,
        );

        assert!(args.contains(&"-IncludeJson".to_string()));
        assert!(args.contains(&"-SkipCommonRoots".to_string()));
    }

    #[test]
    fn deep_scan_args_include_common_root_drilldowns() {
        let args = scanner_script_args(
            Path::new(r"C:\app\Scan-CDriveCleanupAdvisor.ps1"),
            "C",
            Path::new(r"C:\reports\scan"),
            30,
            200,
            true,
        );

        assert!(args.contains(&"-IncludeJson".to_string()));
        assert!(!args.contains(&"-SkipCommonRoots".to_string()));
    }

    #[test]
    fn scanner_progress_parser_accepts_known_markers() {
        let (percent, message) =
            parse_scanner_progress("[WCDCA_PROGRESS] 65|LARGE_FILES").unwrap();

        assert_eq!(percent, 65);
        assert_eq!(message, "正在查找大文件。");
    }

    #[test]
    fn scanner_progress_parser_accepts_heartbeat_markers() {
        let (percent, message) =
            parse_scanner_progress("[WCDCA_PROGRESS] 29|TOP_ROOTS_SCAN:C:\\Users").unwrap();

        assert_eq!(percent, 29);
        assert_eq!(message, "正在统计目录体量：C:\\Users");

        let (percent, message) =
            parse_scanner_progress("[WCDCA_PROGRESS] 68|LARGE_FILES_SCAN:C:\\Program Files")
                .unwrap();

        assert_eq!(percent, 68);
        assert_eq!(message, "正在枚举大文件候选：C:\\Program Files");
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
    fn progress_protocol_fixed_codes_parse_with_frozen_percents() {
        // 固定阶段码携带协议百分比整体过一遍解析器：
        // 常量表与解析分支任何脱节（删码、改码）都在此显式失败。
        let fixed = [
            (PROGRESS_CODE_DRIVE_INFO, PROGRESS_PCT_DRIVE_INFO),
            (PROGRESS_CODE_TOP_ROOTS, PROGRESS_PCT_TOP_ROOTS),
            (PROGRESS_CODE_LARGE_FILES, PROGRESS_PCT_LARGE_FILES),
            (PROGRESS_CODE_SYSTEM_INFO, PROGRESS_PCT_SYSTEM_INFO),
            (PROGRESS_CODE_DISM, PROGRESS_PCT_DISM),
            (PROGRESS_CODE_REPORT, PROGRESS_PCT_REPORT),
            (PROGRESS_CODE_JSON, PROGRESS_PCT_JSON),
        ];
        for (code, pct) in fixed {
            let line = format!(
                "{PROGRESS_MARKER_PREFIX}{pct}{PROGRESS_MARKER_SEPARATOR}{code}"
            );
            let (percent, message) =
                parse_scanner_progress(&line).unwrap_or_else(|| panic!("{code} must parse"));
            assert_eq!(percent, pct, "{code} percent drifted from protocol table");
            assert!(!message.is_empty());
        }
    }

    #[test]
    fn progress_protocol_heartbeat_codes_carry_paths() {
        // 心跳码以冒号接路径；深挖码消息保留“重点目录”措辞（协议消息模板的一部分）。
        let line = format!(
            "{PROGRESS_MARKER_PREFIX}{}{PROGRESS_MARKER_SEPARATOR}{}C:\\Users",
            PROGRESS_PCT_DRILLDOWN_START, PROGRESS_CODE_DRILLDOWN
        );
        let (percent, message) = parse_scanner_progress(&line).unwrap();
        assert_eq!(percent, PROGRESS_PCT_DRILLDOWN_START);
        assert_eq!(message, "正在扫描重点目录：C:\\Users");
    }

    #[test]
    fn resource_scanner_candidates_prefer_script_folder_resource() {
        let root = Path::new("C:\\Program Files\\WindowsCDriveCleanupAdvisor");
        let candidates = resource_scanner_candidates(root);

        assert_eq!(
            candidates[0],
            root.join("scripts").join("Scan-CDriveCleanupAdvisor.ps1")
        );
        assert_eq!(candidates[1], root.join("Scan-CDriveCleanupAdvisor.ps1"));
        assert!(candidates.contains(
            &root
                .join("_up_")
                .join("scripts")
                .join("Scan-CDriveCleanupAdvisor.ps1")
        ));
    }

    #[test]
    fn resource_scanner_candidates_include_sibling_unpack_dir() {
        let root = Path::new("C:\\Program Files\\WindowsCDriveCleanupAdvisor\\resources");
        let candidates = resource_scanner_candidates(root);

        assert!(candidates.contains(
            &Path::new("C:\\Program Files\\WindowsCDriveCleanupAdvisor")
                .join("_up_")
                .join("scripts")
                .join("Scan-CDriveCleanupAdvisor.ps1")
        ));
    }

    #[test]
    fn decode_scanner_line_prefers_utf8_and_falls_back_to_gbk() {
        // UTF-8 中文优先原样解码
        assert_eq!(decode_scanner_line("进度：中文".as_bytes()), "进度：中文");
        // GBK 编码的“中文”(D6 D0 CE C4) 不是合法 UTF-8，须按 GBK 还原
        assert_eq!(decode_scanner_line(&[0xD6, 0xD0, 0xCE, 0xC4]), "中文");
    }

    #[test]
    fn stdout_reader_survives_non_utf8_lines_and_keeps_parsing() {
        let mut data = Vec::new();
        data.extend_from_slice(b"[WCDCA_PROGRESS] 12|TOP_ROOTS_SCAN:ascii\n");
        // GBK 中文路径行（非合法 UTF-8）不得终止读取
        data.extend_from_slice(b"[WCDCA_PROGRESS] 18|TOP_ROOTS_SCAN:C:\\\xD6\xD0\xCE\xC4\\bin\n");
        data.extend_from_slice(b"[WCDCA_PROGRESS] 34|LARGE_FILES_SCAN\n");
        let mut seen = Vec::new();
        for_each_stdout_line(&data[..], |line| {
            seen.push(line.to_string());
            true
        });

        assert_eq!(seen.len(), 3, "非 UTF-8 行不得终止后续行读取");
        for line in &seen {
            assert!(
                parse_scanner_progress(line).is_some(),
                "进度行应仍可解析：{line}"
            );
        }
        assert_eq!(parse_scanner_progress(&seen[1]).unwrap().0, 18);
        assert!(
            seen[1].contains("中文"),
            "GBK 路径应被还原而不是替换符：{}",
            seen[1]
        );
    }

    #[cfg(windows)]
    #[test]
    fn wait_child_with_watchdog_reaps_exiting_child() {
        let shell = resolve_powershell();
        let mut child = Command::new(&shell)
            .arg("-NoProfile")
            .arg("-Command")
            .arg("exit 0")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let activity = Arc::new(Mutex::new(StdInstant::now()));

        let (status, stalled) = wait_child_with_watchdog(
            &mut child,
            &activity,
            Duration::from_secs(60),
            Duration::from_millis(50),
        )
        .unwrap();

        assert!(!stalled);
        assert!(status.success());
    }

    #[cfg(windows)]
    #[test]
    fn wait_child_with_watchdog_kills_stalled_child() {
        let shell = resolve_powershell();
        let mut child = Command::new(&shell)
            .arg("-NoProfile")
            .arg("-Command")
            .arg("Start-Sleep -Seconds 60")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        // 模拟“进度早已停滞”：把最后活动时间拨回过去。
        // Windows 上 Instant 的可回拨上界约等于系统运行时长，
        // 开机不足 1 小时的机器/CI 上直接减 3600 秒会 panic，故用 checked_sub 兜底。
        let backdated = StdInstant::now()
            .checked_sub(Duration::from_secs(3600))
            .unwrap_or_else(StdInstant::now);
        let activity = Arc::new(Mutex::new(backdated));

        let (status, stalled) = wait_child_with_watchdog(
            &mut child,
            &activity,
            Duration::from_secs(1),
            Duration::from_millis(50),
        )
        .unwrap();

        assert!(stalled);
        assert!(!status.success());
    }

    #[cfg(windows)]
    #[test]
    fn wait_child_with_watchdog_does_not_kill_live_child_with_output() {
        let shell = resolve_powershell();
        let mut child = Command::new(&shell)
            .arg("-NoProfile")
            .arg("-Command")
            .arg("1..16 | ForEach-Object { Write-Output \"beat-$_\"; Start-Sleep -Milliseconds 250 }; exit 0")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let activity = Arc::new(Mutex::new(StdInstant::now()));
        let stdout = child.stdout.take().unwrap();
        let stamp = activity.clone();
        let reader = thread::spawn(move || {
            for_each_stdout_line(stdout, move |_| {
                if let Ok(mut s) = stamp.lock() {
                    *s = StdInstant::now();
                }
                true
            })
        });

        // 卡死阈值（2 秒）显著小于子进程总时长（约 4 秒）：
        // 只要读取线程持续用真实输出刷新活性，看门狗就不得误杀；
        // 若活性刷新路径断裂（例如读取线程因编码提前退出），本测试将被判 stalled 而失败。
        let (status, stalled) = wait_child_with_watchdog(
            &mut child,
            &activity,
            Duration::from_millis(2000),
            Duration::from_millis(50),
        )
        .unwrap();
        let _ = reader.join();

        assert!(!stalled, "健康输出的子进程不得被看门狗误杀");
        assert!(status.success());
    }
}
