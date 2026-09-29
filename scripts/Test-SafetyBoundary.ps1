param()

$ErrorActionPreference = "Stop"

# 共享助手（Assert-True 等）：唯一定义在 scripts/Common.ps1。
. "$PSScriptRoot\Common.ps1"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$ScannerPath = Join-Path $RepoRoot "scripts\Scan-CDriveCleanupAdvisor.ps1"
$ScriptsDir = Join-Path $RepoRoot "scripts"
$RustSrcDir = Join-Path $RepoRoot "src-tauri\src"
$CapabilityPath = Join-Path $RepoRoot "src-tauri\capabilities\default.json"
$PackagePath = Join-Path $RepoRoot "package.json"
$CargoPath = Join-Path $RepoRoot "src-tauri\Cargo.toml"
$TauriConfigPath = Join-Path $RepoRoot "src-tauri\tauri.conf.json"

function Assert-FileDoesNotContain {
  param([string]$Path, [string[]]$Patterns, [string]$Label)
  $content = Get-Content -Raw -Encoding UTF8 -LiteralPath $Path
  foreach ($pattern in $Patterns) {
    if ($content -match $pattern) {
      throw "$Label contains forbidden pattern: $pattern"
    }
  }
}

# 产品扫描器黑名单：任何文件系统/系统状态修改动词都不得出现在扫描脚本里。
# 正则按行首（允许缩进）锚定，避免把注释或字符串描述误判为实际命令。
$forbiddenScannerPatterns = @(
  '(?im)^\s*Remove-Item\b',
  '(?im)^\s*Move-Item\b',
  '(?im)^\s*Rename-Item\b',
  '(?im)^\s*Clear-Content\b',
  '(?im)^\s*Set-ItemProperty\b',
  '(?im)^\s*New-ItemProperty\b',
  '(?im)^\s*Remove-ItemProperty\b',
  '(?im)^\s*Stop-Service\b',
  '(?im)^\s*Start-Service\b',
  '(?im)^\s*Restart-Service\b',
  '(?im)^\s*Disable-WindowsOptionalFeature\b',
  '(?im)^\s*Enable-WindowsOptionalFeature\b',
  '(?im)^\s*powercfg\b',
  '(?im)^\s*bcdedit\b',
  '(?im)^\s*diskpart\b',
  '(?im)^\s*format\b',
  '(?im)^\s*takeown\b',
  '(?im)^\s*icacls\b',
  '(?im)^\s*reg\s+',
  '(?im)^\s*sc\s+'
)

$destructiveVerbPattern = '(?im)^\s*(Remove-Item|Move-Item|Rename-Item|Clear-Content|Set-ItemProperty|New-ItemProperty|Remove-ItemProperty|Stop-Service|Start-Service|Restart-Service|Disable-WindowsOptionalFeature|Enable-WindowsOptionalFeature|powercfg|bcdedit|diskpart|format|takeown|icacls|reg\s+|sc\s+)'

# 辅助脚本（打包/测试工具）允许清理的对象白名单：仅限脚本自建的临时/暂存产物变量。
# 命中破坏性动词却未引用任一白名单目标即判失败，防止打包脚本演化出触碰用户数据的能力。
$allowedCleanupTargets = @(
  '$stageDir',
  '$zipPath',
  '$extractRoot',
  '$scanRoot',
  '$tempRoot'
)

$forbiddenShellPluginPatterns = @(
  'tauri-plugin-shell',
  '@tauri-apps/plugin-shell',
  '"shell:',
  'shell:allow',
  'shell:default'
)

Assert-True (Test-Path -LiteralPath $ScannerPath) "Scanner script is missing."
Assert-True (Test-Path -LiteralPath $RustSrcDir) "Rust src-tauri/src directory is missing."
Assert-True (Test-Path -LiteralPath $CapabilityPath) "Tauri capability file is missing."
Assert-True (Test-Path -LiteralPath $PackagePath) "package.json is missing."
Assert-True (Test-Path -LiteralPath $CargoPath) "Cargo.toml is missing."
Assert-True (Test-Path -LiteralPath $TauriConfigPath) "tauri.conf.json is missing."

# 1) 产品扫描器：破坏性动词零容忍（与历史断言一致，覆盖面不减）。
Assert-FileDoesNotContain -Path $ScannerPath -Patterns $forbiddenScannerPatterns -Label "Scanner"

# 2) 其余 ps1 脚本：破坏性动词仅允许清理脚本自建临时产物。
$auxScripts = Get-ChildItem -LiteralPath $ScriptsDir -Filter "*.ps1" |
  Where-Object { $_.Name -ne "Scan-CDriveCleanupAdvisor.ps1" }
foreach ($script in $auxScripts) {
  $lines = Get-Content -Encoding UTF8 -LiteralPath $script.FullName
  for ($index = 0; $index -lt $lines.Count; $index++) {
    $line = $lines[$index]
    if ($line -notmatch $destructiveVerbPattern) {
      continue
    }
    $allowed = $false
    foreach ($target in $allowedCleanupTargets) {
      if ($line.Contains($target)) {
        $allowed = $true
        break
      }
    }
    Assert-True $allowed "$($script.Name) line $($index + 1) uses a destructive verb outside self-owned temp artifacts: $line"
  }
}

# 3) 双内核边界（Phase 2）：原生扫描内核独立于 PowerShell 且保持只读；
#    PS 扫描器降级为兜底路径，由下方原启动断言继续看护。
$KernelPath = Join-Path $RustSrcDir "kernel.rs"
$LibPath = Join-Path $RustSrcDir "lib.rs"
$WcdcaScanPath = Join-Path $RustSrcDir "bin\wcdca-scan.rs"
Assert-True (Test-Path -LiteralPath $KernelPath) "Native scan kernel kernel.rs is missing."
Assert-True (Test-Path -LiteralPath $LibPath) "Rust lib.rs is missing."
Assert-True (Test-Path -LiteralPath $WcdcaScanPath) "wcdca-scan test binary source is missing."

$kernelText = Get-Content -Raw -Encoding UTF8 -LiteralPath $KernelPath
$libText = Get-Content -Raw -Encoding UTF8 -LiteralPath $LibPath
Assert-True ($libText.Contains('pub mod kernel;')) "lib.rs must declare the native kernel module."

# 3.1 进程启动面白名单：产品代码只允许启动 Dism.exe（只读 AnalyzeComponentStore）；
#     测试模块可用 cmd/mklink 建 junction，不在看护面内。
$kernelProduct = ($kernelText -split '#\[cfg\(test\)\]')[0]
foreach ($launch in [regex]::Matches($kernelProduct, 'Command::new\("([^"]+)"\)')) {
  Assert-True ($launch.Groups[1].Value -eq "Dism.exe") "kernel.rs launches a process other than Dism.exe: $($launch.Groups[1].Value)"
}
# 3.2 原生内核不得接入 PS 扫描脚本启动面（注释/报告文案可以提及 PowerShell）。
foreach ($pattern in @('resolve_scanner_script', 'scanner_script_args', '-NoProfile', '-ExecutionPolicy', 'taskkill')) {
  Assert-True ($kernelText -notmatch [regex]::Escape($pattern)) "kernel.rs must stay independent of the PS scanner launch path: found $pattern"
}
# 3.3 只读内核：非测试区不得出现文件删除/改名 API（测试模块允许清理临时目录）。
foreach ($pattern in @('remove_file', 'remove_dir', 'fs::rename', 'std::fs::rename')) {
  Assert-True ($kernelProduct -notmatch [regex]::Escape($pattern)) "kernel.rs product code must stay read-only: found $pattern"
}
# 3.4 默认内核为原生；PS 兜底必须经环境变量显式选择。
Assert-True ($kernelText.Contains('pub const SCAN_KERNEL_ENV: &str = "WCDCA_SCAN_KERNEL";')) "Kernel selector env var drifted from WCDCA_SCAN_KERNEL."
Assert-True ($kernelText.Contains('pub const SCAN_KERNEL_POWERSHELL: &str = "powershell";')) "Kernel selector PS value drifted from 'powershell'."
$commandsText = Get-Content -Raw -Encoding UTF8 -LiteralPath (Join-Path $RustSrcDir "commands.rs")
Assert-True ($commandsText -match 'select_scan_kernel\(\)\s*==\s*SCAN_KERNEL_POWERSHELL') "PS fallback kernel must be opt-in via env var (default is native)."
# 3.5 wcdca-scan 仅供 A/B 断言与基准，不随产品打包。
$cargoText = Get-Content -Raw -Encoding UTF8 -LiteralPath $CargoPath
Assert-True ($cargoText -match '(?s)\[\[bin\]\]\s*\r?\n\s*name\s*=\s*"wcdca-scan"') "Cargo.toml must declare the wcdca-scan test binary."

Assert-FileDoesNotContain -Path $CapabilityPath -Patterns $forbiddenShellPluginPatterns -Label "Capability"
Assert-FileDoesNotContain -Path $PackagePath -Patterns $forbiddenShellPluginPatterns -Label "package.json"
Assert-FileDoesNotContain -Path $CargoPath -Patterns $forbiddenShellPluginPatterns -Label "Cargo.toml"

$capability = Get-Content -Raw -Encoding UTF8 -LiteralPath $CapabilityPath | ConvertFrom-Json
Assert-True ($capability.permissions -contains "core:default") "Capability must include core:default."
Assert-True (-not ($capability.permissions | Where-Object { $_ -like "shell:*" })) "Capability must not include shell permissions."

$tauriConfig = Get-Content -Raw -Encoding UTF8 -LiteralPath $TauriConfigPath | ConvertFrom-Json
$resourceList = @($tauriConfig.bundle.resources)
Assert-True ($resourceList -contains "../scripts/Scan-CDriveCleanupAdvisor.ps1") "Scanner must be bundled as a resource."
$csp = [string]$tauriConfig.app.security.csp
Assert-True ($csp -match "connect-src ipc: http://ipc\.localhost") "CSP must allow only Tauri IPC event connections."
Assert-True ($csp -notmatch "connect-src[^;]*\*") "CSP must not allow arbitrary connect-src origins."

# 4) Rust 侧（PS 兜底启动面）：按模块目录扫描（Phase 1 拆分后源码分布在 src-tauri/src/*.rs），
#    语义不变——走兜底内核时 Rust 必须以固定参数亲自启动产品扫描器，发布构建只从捆绑资源解析脚本。
$rustFiles = Get-ChildItem -LiteralPath $RustSrcDir -Filter "*.rs" -File
Assert-True ($rustFiles.Count -gt 0) "Rust src-tauri/src has no .rs files."
$rust = ($rustFiles | ForEach-Object { Get-Content -Raw -Encoding UTF8 -LiteralPath $_.FullName }) -join "`n"

Assert-True ($rust.Contains('Command::new(&shell)')) "Rust must own scanner process launch."
Assert-True ($rust.Contains('.arg("-NoProfile")')) "PowerShell launch must use -NoProfile."
Assert-True ($rust.Contains('.arg("-ExecutionPolicy")')) "PowerShell launch must set execution policy explicitly."
Assert-True ($rust.Contains('.arg("Bypass")')) "PowerShell launch must use fixed Bypass argument for bundled script."
Assert-True ($rust.Contains('.arg("-File")')) "PowerShell launch must run the bundled script with -File."
Assert-True ($rust.Contains('Scan-CDriveCleanupAdvisor.ps1')) "Rust must resolve the bundled scanner script."
Assert-True ($rust -match '(?s)#\[cfg\(debug_assertions\)\]\s*(pub\(crate\)\s+)?fn\s+dev_scanner_script_path') "Source-tree scanner fallback must be a debug-only function."
Assert-True ($rust.Contains('resource_scanner_candidates')) "Release builds must resolve the scanner from bundled resources."

# 5) Phase 5 清理边界：全项目唯一删除执行面是 cleanup.rs；
#    其余 Rust 模块要么完全只读，要么仅轮转自家报告产物（report.rs）。
$cleanupPath = Join-Path $RustSrcDir "cleanup.rs"
Assert-True (Test-Path -LiteralPath $cleanupPath) "cleanup.rs (the single delete surface) is missing."
$cleanupText = Get-Content -Raw -Encoding UTF8 -LiteralPath $cleanupPath

# 顶层模块 + wcdca-scan 测试二进制统一纳入负向看护（一次性读入复用）。
$allRustTexts = @($rustFiles | ForEach-Object {
    [pscustomobject]@{ Name = $_.Name; Text = (Get-Content -Raw -Encoding UTF8 -LiteralPath $_.FullName) }
  })
$wcdcaScan = Join-Path $RustSrcDir "bin\wcdca-scan.rs"
if (Test-Path -LiteralPath $wcdcaScan) {
  $allRustTexts += [pscustomobject]@{ Name = "bin\wcdca-scan.rs"; Text = (Get-Content -Raw -Encoding UTF8 -LiteralPath $wcdcaScan) }
}

# 5.1 删除执行器（SHFileOperationW / FO_DELETE）只允许出现在 cleanup.rs。
foreach ($entry in $allRustTexts) {
  if ($entry.Name -eq "cleanup.rs") { continue }
  foreach ($pattern in @('SHFileOperationW', 'SHFileOperation', 'FO_DELETE')) {
    Assert-True (-not $entry.Text.Contains($pattern)) "$($entry.Name) must not contain the delete executor (confined to cleanup.rs): found $pattern"
  }
  # 5.2 remove_file/remove_dir/fs::rename：product 区仅允许 report.rs（自家报告轮转）；
  #     测试区不看护（单测清理自己的临时目录），kernel.rs 的 3.3 断言继续独立生效且不放松。
  if ($entry.Name -ne "report.rs") {
    $product = ($entry.Text -split '#\[cfg\(test\)\]')[0]
    foreach ($pattern in @('remove_file', 'remove_dir', 'fs::rename')) {
      Assert-True (-not $product.Contains($pattern)) "$($entry.Name) product code must stay read-only: found $pattern"
    }
  }
}

# 5.3 cleanup.rs 正向安全标记：回收站优先、删除时刻复判、路径身份复检、plan-first 必须在位。
foreach ($marker in @(
    'FOF_ALLOWUNDO',
    'FO_DELETE',
    'SHQueryRecycleBinW',
    'reclassify_gate',
    'GetFileInformationByHandle',
    'FILE_FLAG_BACKUP_SEMANTICS',
    'build_plan_with',
    'from.push(0)'
  )) {
  Assert-True ($cleanupText.Contains($marker)) "cleanup.rs must keep safety marker: $marker"
}
# 去尾随反斜杠（带尾随反斜杠的 pFrom 会绕过回收站直接永久删除）。
Assert-True ($cleanupText.Contains("ends_with('\\')")) "cleanup.rs must strip trailing backslashes before recycle."
# plan-first：execute 必经 build_plan_with（与计划共用同一套校验）。
Assert-True ($cleanupText -match '(?s)fn execute_with[\s\S]*?let plan = build_plan_with\(') "execute_with must build its plan via build_plan_with (plan-first)."

# 5.4 禁提权：任何模块都不得请求 UAC 提权运行（含 wcdca-scan 测试二进制）。
foreach ($entry in $allRustTexts) {
  foreach ($pattern in @('runas', 'ShellExecute')) {
    Assert-True (-not $entry.Text.Contains($pattern)) "$($entry.Name) must not request elevation: found $pattern"
  }
}
# 产品扫描器与辅助脚本一并看护（本脚本自身含模式文本，自扫必误报，跳过）。
$allPs1 = @((Get-Item -LiteralPath $ScannerPath)) + $auxScripts
foreach ($script in $allPs1) {
  if ($script.Name -eq "Test-SafetyBoundary.ps1") { continue }
  $text = Get-Content -Raw -Encoding UTF8 -LiteralPath $script.FullName
  Assert-True (-not ($text -match '(?i)Verb\s+RunAs|Start-Process.*-Verb')) "$($script.Name) must not request elevation (RunAs)."
}

# 5.5 禁网络：产品代码不得有出网能力（前端已由 CSP connect-src 看护）。
foreach ($entry in $allRustTexts) {
  $product = ($entry.Text -split '#\[cfg\(test\)\]')[0]
  foreach ($pattern in @('TcpStream', 'std::net', 'reqwest', 'hyper::', 'InternetOpen', 'WSAStartup', 'WinHttp')) {
    Assert-True (-not $product.Contains($pattern)) "$($entry.Name) product code must not open network access: found $pattern"
  }
}
foreach ($script in $allPs1) {
  if ($script.Name -eq "Test-SafetyBoundary.ps1") { continue }
  $text = Get-Content -Raw -Encoding UTF8 -LiteralPath $script.FullName
  foreach ($pattern in @('Invoke-WebRequest', 'Invoke-RestMethod', 'Net.WebClient', 'bitsadmin')) {
    Assert-True (-not ($text -match "(?i)$pattern")) "$($script.Name) must not open network access: found $pattern"
  }
}

Write-Output "[OK] Safety boundary checks passed"
