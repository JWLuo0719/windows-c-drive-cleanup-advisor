param(
  [string[]]$ArtifactPath = @(".\src-tauri\target\release\windows-c-drive-cleanup-advisor.exe"),
  [string]$OutputPath = ".\dist\checksums.txt"
)

$ErrorActionPreference = "Stop"

function Get-Sha256Hex {
  param(
    [Parameter(Mandatory = $true)]
    [string]$LiteralPath
  )

  $stream = [System.IO.File]::OpenRead($LiteralPath)
  $sha256 = [System.Security.Cryptography.SHA256]::Create()
  try {
    $hashBytes = $sha256.ComputeHash($stream)
    return [System.BitConverter]::ToString($hashBytes).Replace("-", "")
  }
  finally {
    $sha256.Dispose()
    $stream.Dispose()
  }
}

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
  $fileName = Split-Path -Leaf $artifact
  $lines += "SHA256  $hash  $fileName"
}

$lines | Set-Content -LiteralPath $OutputPath -Encoding UTF8

Write-Output "[OK] Checksum written to: $OutputPath"
$lines | ForEach-Object { Write-Output $_ }
