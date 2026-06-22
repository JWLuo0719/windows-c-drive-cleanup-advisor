param(
  [string[]]$ArtifactPath = @(".\src-tauri\target\release\windows-c-drive-cleanup-advisor.exe"),
  [string]$OutputPath = ".\dist\checksums.txt"
)

$ErrorActionPreference = "Stop"

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

  $hash = Get-FileHash -LiteralPath $artifact -Algorithm SHA256
  $fileName = Split-Path -Leaf $artifact
  $lines += "SHA256  $($hash.Hash)  $fileName"
}

$lines | Set-Content -LiteralPath $OutputPath -Encoding UTF8

Write-Output "[OK] Checksum written to: $OutputPath"
$lines | ForEach-Object { Write-Output $_ }
