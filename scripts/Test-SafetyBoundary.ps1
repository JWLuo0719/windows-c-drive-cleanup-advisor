param()

$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$ScannerPath = Join-Path $RepoRoot "scripts\Scan-CDriveCleanupAdvisor.ps1"
$CapabilityPath = Join-Path $RepoRoot "src-tauri\capabilities\default.json"
$PackagePath = Join-Path $RepoRoot "package.json"
$CargoPath = Join-Path $RepoRoot "src-tauri\Cargo.toml"
$TauriConfigPath = Join-Path $RepoRoot "src-tauri\tauri.conf.json"
$RustLibPath = Join-Path $RepoRoot "src-tauri\src\lib.rs"

function Assert-True {
  param([bool]$Condition, [string]$Message)
  if (-not $Condition) {
    throw $Message
  }
}

function Assert-FileDoesNotContain {
  param([string]$Path, [string[]]$Patterns, [string]$Label)
  $content = Get-Content -Raw -Encoding UTF8 -LiteralPath $Path
  foreach ($pattern in $Patterns) {
    if ($content -match $pattern) {
      throw "$Label contains forbidden pattern: $pattern"
    }
  }
}

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

$forbiddenShellPluginPatterns = @(
  'tauri-plugin-shell',
  '@tauri-apps/plugin-shell',
  '"shell:',
  'shell:allow',
  'shell:default'
)

Assert-True (Test-Path -LiteralPath $ScannerPath) "Scanner script is missing."
Assert-True (Test-Path -LiteralPath $CapabilityPath) "Tauri capability file is missing."
Assert-True (Test-Path -LiteralPath $PackagePath) "package.json is missing."
Assert-True (Test-Path -LiteralPath $CargoPath) "Cargo.toml is missing."
Assert-True (Test-Path -LiteralPath $TauriConfigPath) "tauri.conf.json is missing."
Assert-True (Test-Path -LiteralPath $RustLibPath) "Rust lib.rs is missing."

Assert-FileDoesNotContain -Path $ScannerPath -Patterns $forbiddenScannerPatterns -Label "Scanner"
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

$rust = Get-Content -Raw -Encoding UTF8 -LiteralPath $RustLibPath
Assert-True ($rust.Contains('Command::new(&shell)')) "Rust must own scanner process launch."
Assert-True ($rust.Contains('.arg("-NoProfile")')) "PowerShell launch must use -NoProfile."
Assert-True ($rust.Contains('.arg("-ExecutionPolicy")')) "PowerShell launch must set execution policy explicitly."
Assert-True ($rust.Contains('.arg("Bypass")')) "PowerShell launch must use fixed Bypass argument for bundled script."
Assert-True ($rust.Contains('.arg("-File")')) "PowerShell launch must run the bundled script with -File."
Assert-True ($rust.Contains('Scan-CDriveCleanupAdvisor.ps1')) "Rust must resolve the bundled scanner script."
Assert-True ($rust.Contains('#[cfg(debug_assertions)]')) "Source-tree scanner fallback must be limited to debug builds."
Assert-True ($rust.Contains('resource_scanner_candidates')) "Release builds must resolve the scanner from bundled resources."

Write-Output "[OK] Safety boundary checks passed"

