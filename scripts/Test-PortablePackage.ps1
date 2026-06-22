param(
  [string]$ZipPath = ".\dist\windows-c-drive-cleanup-advisor-0.1.0-windows-x64.zip",
  [string]$ChecksumPath = ".\dist\checksums.txt",
  [switch]$SkipHashValidation
)

$ErrorActionPreference = "Stop"

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
    "docs\RELEASE_CHECKLIST.md"
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
      "windows-c-drive-cleanup-advisor.exe" { ".\src-tauri\target\release\windows-c-drive-cleanup-advisor.exe" }
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

Write-Output "[OK] Portable package validated: $ZipPath"
