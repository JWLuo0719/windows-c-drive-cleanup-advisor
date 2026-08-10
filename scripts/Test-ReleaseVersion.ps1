Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$tauriConfigPath = Join-Path $repoRoot "src-tauri\tauri.conf.json"
$packagePath = Join-Path $repoRoot "package.json"
$manifestPath = Join-Path $repoRoot "src-tauri\Cargo.toml"

$tauriConfig = Get-Content -LiteralPath $tauriConfigPath -Raw -Encoding UTF8 | ConvertFrom-Json
$package = Get-Content -LiteralPath $packagePath -Raw -Encoding UTF8 | ConvertFrom-Json
$tauriVersion = [string]$tauriConfig.version
$packageVersion = [string]$package.version

$metadataText = & cargo metadata --manifest-path $manifestPath --no-deps --format-version 1
if ($LASTEXITCODE -ne 0) {
  throw "cargo metadata failed with exit code $LASTEXITCODE."
}

$metadata = $metadataText | ConvertFrom-Json
$cargoPackage = @($metadata.packages | Where-Object { $_.name -eq "windows-c-drive-cleanup-advisor" }) | Select-Object -First 1
if ($null -eq $cargoPackage) {
  throw "Cargo package metadata was not found."
}
$cargoVersion = [string]$cargoPackage.version

if ([string]::IsNullOrWhiteSpace($tauriVersion)) {
  throw "Tauri release version is empty."
}
if ($packageVersion -ne $tauriVersion) {
  throw "package.json version $packageVersion does not match Tauri version $tauriVersion."
}
if ($cargoVersion -ne $tauriVersion) {
  throw "Cargo version $cargoVersion does not match Tauri version $tauriVersion."
}

Write-Output "[OK] Release versions match: $tauriVersion"
