param(
  [string[]]$ArtifactPath = @(".\src-tauri\target\release\windows-c-drive-cleanup-advisor.exe"),
  [string]$OutputPath = ".\dist\checksums.txt"
)

$ErrorActionPreference = "Stop"

# 共享助手（Get-Sha256Hex）：唯一定义在 scripts/Common.ps1。
. "$PSScriptRoot\Common.ps1"

$outputDir = Split-Path -Parent $OutputPath
if ($outputDir -and (-not (Test-Path -LiteralPath $outputDir))) {
  New-Item -ItemType Directory -Force -Path $outputDir | Out-Null
}

$artifacts = @()
foreach ($artifact in $ArtifactPath) {
  $artifacts += ($artifact -split "," | ForEach-Object { $_.Trim() } | Where-Object { $_ })
}

$lines = @()
foreach ($artifact in $artifacts) {
  if (-not (Test-Path -LiteralPath $artifact)) {
    throw "Artifact not found: $artifact"
  }

  $hash = Get-Sha256Hex -LiteralPath $artifact
  # GNU sha256sum 文本格式（hash 两空格 相对路径），在仓库根可直接 `sha256sum -c dist/checksums.txt`。
  $relativePath = (Resolve-Path -LiteralPath $artifact).Path.Substring((Get-Location).Path.Length + 1) -replace "\\", "/"
  $lines += "$hash  $relativePath"
}

# UTF-8 无 BOM + LF 行尾：sha256sum -c 要求首行不得有 BOM，CRLF 也会干扰解析。
$utf8NoBom = New-Object System.Text.UTF8Encoding($false)
$text = ($lines -join "`n") + "`n"
[System.IO.File]::WriteAllText($OutputPath, $text, $utf8NoBom)

Write-Output "[OK] Checksum written to: $OutputPath"
$lines | ForEach-Object { Write-Output $_ }
