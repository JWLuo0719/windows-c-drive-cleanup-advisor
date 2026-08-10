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
  $global:LASTEXITCODE = 0
  & $Action
  $exitCode = $LASTEXITCODE
  if ($exitCode -ne 0) {
    throw "$Name failed with exit code $exitCode."
  }
  Write-Output "[OK] $Name"
}

Push-Location $repoRoot
try {
  $tauriConfig = Get-Content -LiteralPath ".\src-tauri\tauri.conf.json" -Raw -Encoding UTF8 | ConvertFrom-Json
  $releaseVersion = [string]$tauriConfig.version

  if ($Install) {
    Invoke-Step "Install npm dependencies" {
      npm ci
    }
  }

  Invoke-Step "Validate release version contract" {
    powershell -NoProfile -ExecutionPolicy Bypass -File ".\scripts\Test-ReleaseVersion.ps1"
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
      powershell -NoProfile -ExecutionPolicy Bypass -File ".\scripts\New-PortablePackage.ps1" -Version $releaseVersion
    }
  }

  if ((-not $SkipChecksum) -and (-not $SkipTauriBuild)) {
    Invoke-Step "Generate release checksum" {
      $artifacts = @(".\src-tauri\target\release\windows-c-drive-cleanup-advisor.exe")
      if (-not $SkipPackage) {
        $artifacts += ".\dist\windows-c-drive-cleanup-advisor-$releaseVersion-windows-x64.zip"
      }
      powershell -NoProfile -ExecutionPolicy Bypass -File ".\scripts\New-ReleaseChecksum.ps1" -ArtifactPath ($artifacts -join ",")
    }
  }

  if ((-not $SkipPackage) -and (-not $SkipChecksum) -and (-not $SkipTauriBuild)) {
    Invoke-Step "Validate portable release package" {
      $zipPath = ".\dist\windows-c-drive-cleanup-advisor-$releaseVersion-windows-x64.zip"
      powershell -NoProfile -ExecutionPolicy Bypass -File ".\scripts\Test-PortablePackage.ps1" -ZipPath $zipPath
    }
  }
}
finally {
  Pop-Location
}

