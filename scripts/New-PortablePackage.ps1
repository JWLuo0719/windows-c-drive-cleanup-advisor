param(
  [string]$Version = "",
  [string]$ReleaseDir = ".\src-tauri\target\release",
  [string]$DistDir = ".\dist"
)

$ErrorActionPreference = "Stop"

if ([string]::IsNullOrWhiteSpace($Version)) {
  $repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
  $tauriConfigPath = Join-Path $repoRoot "src-tauri\tauri.conf.json"
  $tauriConfig = Get-Content -LiteralPath $tauriConfigPath -Raw -Encoding UTF8 | ConvertFrom-Json
  $Version = [string]$tauriConfig.version
}

$exePath = Join-Path $ReleaseDir "windows-c-drive-cleanup-advisor.exe"
$resourceDir = Join-Path $ReleaseDir "_up_"

if (-not (Test-Path -LiteralPath $exePath)) {
  throw "Release executable not found: $exePath"
}

if (-not (Test-Path -LiteralPath $resourceDir)) {
  throw "Tauri resource directory not found: $resourceDir"
}

if (-not (Test-Path -LiteralPath $DistDir)) {
  New-Item -ItemType Directory -Force -Path $DistDir | Out-Null
}

$packageName = "windows-c-drive-cleanup-advisor-$Version-windows-x64"
$stageDir = Join-Path $DistDir $packageName
$zipPath = Join-Path $DistDir "$packageName.zip"

if (Test-Path -LiteralPath $stageDir) {
  Remove-Item -LiteralPath $stageDir -Recurse -Force
}

if (Test-Path -LiteralPath $zipPath) {
  Remove-Item -LiteralPath $zipPath -Force
}

New-Item -ItemType Directory -Force -Path $stageDir | Out-Null
Copy-Item -LiteralPath $exePath -Destination $stageDir
Copy-Item -LiteralPath $resourceDir -Destination $stageDir -Recurse

foreach ($doc in @("README.md", "USER-GUIDE.md", "AGENT.md", "LICENSE")) {
  if (Test-Path -LiteralPath $doc) {
    Copy-Item -LiteralPath $doc -Destination $stageDir
  }
}

foreach ($doc in @(
  "docs\README.md",
  "docs\release\RELEASE_CHECKLIST_$Version.md",
  "docs\product\PRODUCT_PLAN.md",
  "docs\release\RELEASE_NOTES_$Version.md",
  "docs\release\SMOKE_TEST_REPORT_TEMPLATE.md"
)) {
  if (Test-Path -LiteralPath $doc) {
    $docDestinationDir = Join-Path $stageDir (Split-Path -Parent $doc)
    New-Item -ItemType Directory -Force -Path $docDestinationDir | Out-Null
    Copy-Item -LiteralPath $doc -Destination $docDestinationDir
  }
}

Compress-Archive -Path (Join-Path $stageDir "*") -DestinationPath $zipPath -Force

Write-Output "[OK] Portable package written to: $zipPath"
Write-Output $zipPath
