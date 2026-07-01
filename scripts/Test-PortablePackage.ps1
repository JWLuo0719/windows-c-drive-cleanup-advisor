param(
  [string]$ZipPath = ".\dist\windows-c-drive-cleanup-advisor-0.1.0-windows-x64.zip",
  [string]$ChecksumPath = ".\dist\checksums.txt",
  [string]$ReleaseDir = ".\src-tauri\target\release",
  [switch]$SkipHashValidation,
  [switch]$SkipRuntimeSmoke
)

$ErrorActionPreference = "Stop"

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

if (-not (Test-Path -LiteralPath $ZipPath)) {
  throw "Portable package not found: $ZipPath"
}

if (-not (Test-Path -LiteralPath $ChecksumPath)) {
  throw "Checksum file not found: $ChecksumPath"
}

Add-Type -AssemblyName System.IO.Compression.FileSystem

$zip = [System.IO.Compression.ZipFile]::OpenRead((Resolve-Path $ZipPath))
try {
  $entries = @($zip.Entries | ForEach-Object { $_.FullName -replace "/", "\" })
  $requiredEntries = @(
    "windows-c-drive-cleanup-advisor.exe",
    "_up_\scripts\Scan-CDriveCleanupAdvisor.ps1",
    "README.md",
    "USER-GUIDE.md",
    "AGENT.md",
    "docs\RELEASE_CHECKLIST.md",
    "docs\PRODUCT_PLAN.md",
    "docs\RELEASE_NOTES_0.1.0.md",
    "docs\SMOKE_TEST_REPORT_TEMPLATE.md"
  )

  foreach ($entry in $requiredEntries) {
    if ($entries -notcontains $entry) {
      throw "Portable package is missing required entry: $entry"
    }
  }
}
finally {
  $zip.Dispose()
}

$checksumText = Get-Content -LiteralPath $ChecksumPath -Raw
$requiredArtifacts = @(
  "windows-c-drive-cleanup-advisor.exe",
  (Split-Path -Leaf $ZipPath)
)

foreach ($artifact in $requiredArtifacts) {
  if ($checksumText -notmatch [regex]::Escape($artifact)) {
    throw "Checksum file is missing artifact: $artifact"
  }
}

if (-not $SkipHashValidation) {
  $checksumLines = Get-Content -LiteralPath $ChecksumPath
  foreach ($line in $checksumLines) {
    if (-not $line.Trim()) {
      continue
    }

    $parts = $line -split "\s+"
    if ($parts.Count -lt 3) {
      throw "Invalid checksum line: $line"
    }

    $expectedHash = $parts[1]
    $fileName = $parts[2]
    $candidate = switch ($fileName) {
      "windows-c-drive-cleanup-advisor.exe" { Join-Path $ReleaseDir "windows-c-drive-cleanup-advisor.exe" }
      default { Join-Path (Split-Path -Parent $ZipPath) $fileName }
    }

    if (-not (Test-Path -LiteralPath $candidate)) {
      throw "Checksum artifact not found: $candidate"
    }

    $actualHash = (Get-FileHash -LiteralPath $candidate -Algorithm SHA256).Hash
    if ($actualHash -ne $expectedHash) {
      throw "Checksum mismatch for ${fileName}: expected $expectedHash but got $actualHash"
    }
  }
}

if (-not $SkipRuntimeSmoke) {
  $extractRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("wcdca-portable-runtime-" + [guid]::NewGuid().ToString("N"))
  $scanRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("wcdca-portable-scan-" + [guid]::NewGuid().ToString("N"))
  $reportDir = Join-Path $scanRoot "reports"
  $driveLetter = Get-FreeDriveLetter
  $driveName = "${driveLetter}:"
  $substCreated = $false

  try {
    [System.IO.Compression.ZipFile]::ExtractToDirectory((Resolve-Path $ZipPath), $extractRoot)

    $portableExe = Join-Path $extractRoot "windows-c-drive-cleanup-advisor.exe"
    $portableScanner = Join-Path $extractRoot "_up_\scripts\Scan-CDriveCleanupAdvisor.ps1"
    Assert-True (Test-Path -LiteralPath $portableExe) "Extracted portable executable is missing."
    Assert-True (Test-Path -LiteralPath $portableScanner) "Extracted bundled scanner script is missing."

    New-Item -ItemType Directory -Force -Path $scanRoot | Out-Null
    New-Item -ItemType Directory -Force -Path (Join-Path $scanRoot "CacheRoot") | Out-Null
    New-Item -ItemType Directory -Force -Path $reportDir | Out-Null

    $largeFile = Join-Path $scanRoot "CacheRoot\large-cache.bin"
    $stream = [System.IO.File]::Open($largeFile, [System.IO.FileMode]::Create, [System.IO.FileAccess]::Write)
    try {
      $stream.SetLength(2MB)
    }
    finally {
      $stream.Dispose()
    }

    subst $driveName $scanRoot
    if ($LASTEXITCODE -ne 0) {
      throw "subst failed for $driveName"
    }
    $substCreated = $true

    $scanOutput = & powershell -NoProfile -ExecutionPolicy Bypass -File $portableScanner `
      -Drive $driveLetter `
      -OutputDir $reportDir `
      -TopCount 5 `
      -LargeFileMB 1 `
      -IncludeJson `
      -SkipCommonRoots

    $scanText = $scanOutput -join "`n"
    Assert-True ($scanText -match "\[WCDCA_PROGRESS\]") "Extracted scanner did not emit progress markers."
    Assert-True ($scanText -match "TOP_ROOTS_SCAN") "Extracted scanner did not emit top-root heartbeat markers."
    Assert-True ($scanText -match "LARGE_FILES_SCAN") "Extracted scanner did not emit large-file heartbeat markers."
    Assert-True ($scanText -match "\[OK\] Report written") "Extracted scanner did not report Markdown output."
    Assert-True ($scanText -match "\[OK\] JSON written") "Extracted scanner did not report JSON output."
    Assert-True (Test-Path -LiteralPath $largeFile) "Extracted scanner changed or removed the source file."

    $jsonPath = Get-ChildItem -LiteralPath $reportDir -Filter "*.json" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    $markdownPath = Get-ChildItem -LiteralPath $reportDir -Filter "*.md" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
    Assert-True ($null -ne $jsonPath) "Extracted scanner did not create a JSON report."
    Assert-True ($null -ne $markdownPath) "Extracted scanner did not create a Markdown report."

    $json = Get-Content -Raw -Encoding UTF8 -LiteralPath $jsonPath.FullName | ConvertFrom-Json
    Assert-True ($json.drive -eq "${driveLetter}:\") "Extracted scanner JSON drive does not match temporary drive."
    $largeFiles = @($json.largeFiles)
    Assert-True ($largeFiles.Count -ge 1) "Extracted scanner JSON did not include the seeded large file."
  }
  finally {
    if ($substCreated) {
      subst $driveName /D | Out-Null
    }
    if (Test-Path -LiteralPath $extractRoot) {
      Remove-Item -LiteralPath $extractRoot -Recurse -Force
    }
    if (Test-Path -LiteralPath $scanRoot) {
      Remove-Item -LiteralPath $scanRoot -Recurse -Force
    }
  }
}

Write-Output "[OK] Portable package validated: $ZipPath"
