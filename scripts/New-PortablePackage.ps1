param(
  [string]$Version = "0.1.0",
  [string]$ReleaseDir = ".\src-tauri\target\release",
  [string]$DistDir = ".\dist"
)

$ErrorActionPreference = "Stop"

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

foreach ($doc in @("README.md", "USER-GUIDE.md")) {
  if (Test-Path -LiteralPath $doc) {
    Copy-Item -LiteralPath $doc -Destination $stageDir
  }
}

$docsStageDir = Join-Path $stageDir "docs"
New-Item -ItemType Directory -Force -Path $docsStageDir | Out-Null
foreach ($doc in @("docs\RELEASE_CHECKLIST.md")) {
  if (Test-Path -LiteralPath $doc) {
    Copy-Item -LiteralPath $doc -Destination $docsStageDir
  }
}

Compress-Archive -Path (Join-Path $stageDir "*") -DestinationPath $zipPath -Force

Write-Output "[OK] Portable package written to: $zipPath"
Write-Output $zipPath
