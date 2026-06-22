param(
  [switch]$Install,
  [switch]$SkipTauriBuild,
  [switch]$SkipPackage,
  [switch]$SkipChecksum
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")

function Invoke-Step {
  param(
    [string]$Name,
    [scriptblock]$Action
  )

  Write-Output "[CHECK] $Name"
  & $Action
  Write-Output "[OK] $Name"
}

Push-Location $repoRoot
try {
  if ($Install) {
    Invoke-Step "Install npm dependencies" {
      npm ci
    }
  }

  Invoke-Step "Check read-only safety boundary" {
    powershell -NoProfile -ExecutionPolicy Bypass -File ".\scripts\Test-SafetyBoundary.ps1"
  }

  Invoke-Step "Check scanner output contract" {
    powershell -NoProfile -ExecutionPolicy Bypass -File ".\scripts\Test-ScannerContract.ps1"
  }

  Invoke-Step "Run frontend unit tests" {
    npm test
  }

  Invoke-Step "Build frontend" {
    npm run build
  }

  Invoke-Step "Audit npm dependencies" {
    npm audit --audit-level=moderate
  }

  Invoke-Step "Run Rust unit tests" {
    Push-Location "src-tauri"
    try {
      cargo test
    }
    finally {
      Pop-Location
    }
  }

  Invoke-Step "Check Rust build" {
    Push-Location "src-tauri"
    try {
      cargo check
    }
    finally {
      Pop-Location
    }
  }

  if (-not $SkipTauriBuild) {
    Invoke-Step "Build Tauri release executable" {
      npm run tauri:build
    }
  }

  if ((-not $SkipPackage) -and (-not $SkipTauriBuild)) {
    Invoke-Step "Create portable release package" {
      powershell -NoProfile -ExecutionPolicy Bypass -File ".\scripts\New-PortablePackage.ps1"
    }
  }

  if ((-not $SkipChecksum) -and (-not $SkipTauriBuild)) {
    Invoke-Step "Generate release checksum" {
      $artifacts = @(".\src-tauri\target\release\windows-c-drive-cleanup-advisor.exe")
      if (-not $SkipPackage) {
        $artifacts += ".\dist\windows-c-drive-cleanup-advisor-0.1.0-windows-x64.zip"
      }
      powershell -NoProfile -ExecutionPolicy Bypass -File ".\scripts\New-ReleaseChecksum.ps1" -ArtifactPath ($artifacts -join ",")
    }
  }

  if ((-not $SkipPackage) -and (-not $SkipChecksum) -and (-not $SkipTauriBuild)) {
    Invoke-Step "Validate portable release package" {
      powershell -NoProfile -ExecutionPolicy Bypass -File ".\scripts\Test-PortablePackage.ps1"
    }
  }
}
finally {
  Pop-Location
}

