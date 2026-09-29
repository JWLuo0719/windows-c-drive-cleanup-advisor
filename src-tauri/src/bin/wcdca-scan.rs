//! 原生扫描内核命令行入口（测试/基准专用，不随产品打包）。
//!
//! 输出与 PowerShell 扫描器同构的原始 Markdown/JSON 报告，并把
//! `[WCDCA_PROGRESS] {percent}|{code}` 标记行打到 stdout（协议与 PS 发射端一致），
//! 供 scripts/Test-ScannerContract.ps1 双内核 A/B 等价断言与
//! scripts/Measure-ScanPerformance.ps1 性能闸门使用。
//!
//! 用法：
//!   wcdca-scan --root <dir> --output <dir> [--top N] [--large-mb M]
//!              [--skip-common-roots] [--common-root <dir>]...

use std::path::PathBuf;
use std::process::ExitCode;
use windows_c_drive_cleanup_advisor_lib::kernel::{
    format_marker_line, run_scan, write_raw_reports, KernelError, KernelOptions,
};

fn print_usage() {
    eprintln!(
        "usage: wcdca-scan --root <dir> --output <dir> [--top N] [--large-mb M] [--skip-common-roots] [--common-root <dir>]..."
    );
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut root: Option<String> = None;
    let mut output: Option<String> = None;
    let mut top_count: usize = 30;
    let mut large_file_mb: u64 = 200;
    let mut skip_common_roots = false;
    let mut common_roots: Vec<String> = Vec::new();

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                root = args.get(index).cloned();
            }
            "--output" => {
                index += 1;
                output = args.get(index).cloned();
            }
            "--top" => {
                index += 1;
                top_count = args
                    .get(index)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(30);
            }
            "--large-mb" => {
                index += 1;
                large_file_mb = args
                    .get(index)
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(200);
            }
            "--skip-common-roots" => skip_common_roots = true,
            "--common-root" => {
                index += 1;
                if let Some(value) = args.get(index) {
                    common_roots.push(value.clone());
                }
            }
            "--help" | "-h" => {
                print_usage();
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("unknown argument: {other}");
                print_usage();
                return ExitCode::from(64);
            }
        }
        index += 1;
    }

    let (Some(root), Some(output)) = (root, output) else {
        print_usage();
        return ExitCode::from(64);
    };

    let include_common_roots = !skip_common_roots;
    let mut options = KernelOptions::new(&root, top_count, large_file_mb, include_common_roots);
    options.common_roots = common_roots;

    let progress = |percent: u8, code_with_path: &str| -> bool {
        println!("{}", format_marker_line(percent, code_with_path));
        true
    };

    match run_scan(&options, &progress) {
        Ok(result) => {
            let output_dir = PathBuf::from(&output);
            match write_raw_reports(&result, &output_dir, &progress) {
                Ok((markdown_path, json_path)) => {
                    println!("[OK] Report written to: {}", markdown_path.display());
                    println!("[OK] JSON written to: {}", json_path.display());
                    ExitCode::SUCCESS
                }
                Err(err) => {
                    eprintln!("[ERROR] report write failed: {err}");
                    ExitCode::FAILURE
                }
            }
        }
        Err(KernelError::Cancelled) => {
            eprintln!("[ERROR] scan cancelled");
            ExitCode::from(2)
        }
        Err(KernelError::Failed(message)) => {
            eprintln!("[ERROR] scan failed: {message}");
            ExitCode::FAILURE
        }
    }
}
