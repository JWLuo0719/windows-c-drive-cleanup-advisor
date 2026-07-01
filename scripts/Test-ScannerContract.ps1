param()

$ErrorActionPreference = "Stop"

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$ScannerPath = Join-Path $RepoRoot "scripts\Scan-CDriveCleanupAdvisor.ps1"

function Assert-True {
  param([bool]$Condition, [string]$Message)
  if (-not $Condition) {
    throw $Message
  }
}

function Get-FreeDriveLetter {
  $used = [System.IO.DriveInfo]::GetDrives() | ForEach-Object { $_.Name.Substring(0, 1).ToUpperInvariant() }
  foreach ($letter in @("Z", "Y", "X", "W", "V", "U", "T")) {
    if ($used -notcontains $letter) {
      return $letter
    }
  }
  throw "No temporary drive letter is available."
}

Assert-True (Test-Path -LiteralPath $ScannerPath) "Scanner script is missing."

$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("wcdca-scanner-contract-" + [guid]::NewGuid().ToString("N"))
$reportDir = Join-Path $tempRoot "reports"
$driveLetter = Get-FreeDriveLetter
$driveName = "${driveLetter}:"
$substCreated = $false

try {
  New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null
  New-Item -ItemType Directory -Force -Path (Join-Path $tempRoot "CacheRoot") | Out-Null
  New-Item -ItemType Directory -Force -Path $reportDir | Out-Null

  $largeFile = Join-Path $tempRoot "CacheRoot\large-cache.bin"
  $stream = [System.IO.File]::Open($largeFile, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write)
  try {
    $stream.SetLength(2MB)
  }
  finally {
    $stream.Dispose()
  }

  subst $driveName $tempRoot
  if ($LASTEXITCODE -ne 0) {
    throw "subst failed for $driveName"
  }
  $substCreated = $true

  $scanOutput = & powershell -NoProfile -ExecutionPolicy Bypass -File $ScannerPath `
    -Drive $driveLetter `
    -OutputDir $reportDir `
    -TopCount 5 `
    -LargeFileMB 1 `
    -IncludeJson `
    -SkipCommonRoots

  Assert-True (($scanOutput -join "`n") -match "\[WCDCA_PROGRESS\]") "Scanner did not emit progress markers."
  Assert-True (($scanOutput -join "`n") -match "TOP_ROOTS_SCAN") "Scanner did not emit top-root heartbeat markers."
  Assert-True (($scanOutput -join "`n") -match "LARGE_FILES_SCAN") "Scanner did not emit large-file heartbeat markers."
  Assert-True (($scanOutput -join "`n") -match "\[OK\] Report written") "Scanner did not report Markdown output."
  Assert-True (($scanOutput -join "`n") -match "\[OK\] JSON written") "Scanner did not report JSON output."
  Assert-True (Test-Path -LiteralPath $largeFile) "Scanner changed or removed the source file."

  $jsonPath = Get-ChildItem -LiteralPath $reportDir -Filter "*.json" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
  $markdownPath = Get-ChildItem -LiteralPath $reportDir -Filter "*.md" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
  Assert-True ($null -ne $jsonPath) "Scanner did not create a JSON report."
  Assert-True ($null -ne $markdownPath) "Scanner did not create a Markdown report."

  $json = Get-Content -Raw -Encoding UTF8 -LiteralPath $jsonPath.FullName | ConvertFrom-Json
  Assert-True ($json.drive -eq "${driveLetter}:\") "JSON report drive does not match temporary drive."
  $largeFiles = @($json.largeFiles)
  Assert-True ($largeFiles.Count -ge 1) "JSON report did not include the seeded large file."
  $seededLargeFiles = @($largeFiles | Where-Object {
    ($_.Path -like "$driveName*") -or ($_.Path -like "$tempRoot*")
  })
  Assert-True ($seededLargeFiles.Count -ge 1) "Large file path is not under the temporary scan root."
  $drilldownProperties = @($json.drilldowns.PSObject.Properties | Where-Object { $_.MemberType -eq "NoteProperty" })
  Assert-True ($drilldownProperties.Count -eq 0) "SkipCommonRoots should avoid common-root drilldowns."

  $markdown = Get-Content -Raw -Encoding UTF8 -LiteralPath $markdownPath.FullName
  Assert-True ($markdown.Contains("Read-only scan: yes")) "Markdown report does not state read-only scan."
  Assert-True ($markdown.Contains("This tool did not delete, move, or modify files.")) "Markdown report does not state no file changes."
}
finally {
  if ($substCreated) {
    subst $driveName /D | Out-Null
  }
  if (Test-Path -LiteralPath $tempRoot) {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force
  }
}

Write-Output "[OK] Scanner contract checks passed"
