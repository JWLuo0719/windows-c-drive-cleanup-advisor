# Windows C Drive Cleanup Advisor

A read-only Windows desktop advisor for diagnosing C drive disk pressure. It scans real local paths, skips reparse-point locations, ranks large folders and files, and explains what is safe to review.

## Current Status

Version `0.1.0` is intentionally advisory only:

- Tauri v2 desktop shell with React/Vite UI.
- Rust owns the narrow IPC boundary.
- The bundled PowerShell scanner runs with fixed arguments from Rust.
- Markdown and enriched JSON reports are written locally.
- No cleanup, deletion, move, uninstall, upload, or settings change is performed.

## Run From Source

```powershell
npm install
npm run tauri:dev
```

The legacy script runner is still available:

```powershell
.\Run-CDriveCleanupAdvisor.cmd
```

## Verify Locally

Run the same checks expected by CI:

```powershell
npm run verify
```

For a faster loop without rebuilding the release executable:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\Invoke-ProjectChecks.ps1 -SkipTauriBuild
```

The full verification script runs the read-only safety boundary check, scanner output contract check, frontend tests, frontend build, npm audit, Rust tests, Rust check, Tauri release build, portable packaging, checksum generation, and package validation.
It also validates that the portable zip contains the executable, bundled scanner script, and user-facing docs.

To run only the safety boundary check:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\Test-SafetyBoundary.ps1
```

That check fails if the scanner script contains common destructive PowerShell commands, if the Tauri capability grants shell permissions, if shell plugins are added to package manifests, or if Rust stops launching the bundled scanner with fixed PowerShell arguments.

To run only the scanner output contract check:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\Test-ScannerContract.ps1
```

That check runs the scanner against a temporary drive, confirms progress markers, verifies Markdown and JSON reports, and confirms the source files are not changed.

## CI

GitHub Actions workflow `.github/workflows/ci.yml` runs on Windows. It installs dependencies, runs the same `npm run verify` pipeline used locally, and uploads the release executable, portable zip, and checksum file as artifacts.

Before publishing a GitHub Release, walk through [docs/RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md).

## Safety Boundary

The app is built around a conservative rule: diagnose first, let the user decide. The frontend does not receive generic shell permissions and cannot invoke arbitrary commands. The Rust backend only exposes fixed scan commands.

`scripts\Test-SafetyBoundary.ps1` is part of local and CI verification. It checks that the bundled scanner remains read-only, that no Tauri shell permission is granted to the frontend, and that Rust still owns scanner process launch with fixed arguments.

## What This App Never Deletes

Version `0.1.0` never deletes anything. It also never automatically handles:

- `C:\Windows\WinSxS`
- `C:\Windows\Installer`
- `C:\Windows\System32`
- `C:\System Volume Information`
- `C:\pagefile.sys`, `C:\swapfile.sys`, `C:\hiberfil.sys`
- WSL `ext4.vhdx` files
- WeChat, QQ, or WXWork message stores
- Installed application directories

## Privacy

Reports are written to local disk only. The app does not upload scan output, file paths, telemetry, or usage data.

## Known False Positives

Heuristic categories are advisory. Some cache-looking folders can contain important app state, and some app-managed folders can contain disposable installers. Review paths before acting.

## Roadmap

- `v0.1`: Read-only GUI, risk groups, Markdown/JSON reports.
- `v0.2`: Better progress, cancellation polish, export flow, mock tests.
- `v0.3`: Experimental low-risk cache cleanup allowlist, using `reportId + candidateIds` only and moving items to the recycle bin by default.
- `v1.0`: Signed or clearly unsigned release, installer, portable zip, checksums, complete release verification guide.

## SmartScreen Notice

Unsigned Windows builds can trigger Microsoft Defender SmartScreen warnings. Public releases should either be signed or clearly marked as unsigned.

## Checksums

Release builds should publish SHA-256 checksums next to installers and portable archives.

Create a portable zip from the release executable and bundled Tauri resources:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\New-PortablePackage.ps1
```

Generate a checksum for the local release executable:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\New-ReleaseChecksum.ps1
```

The full `npm run verify` flow creates:

- `src-tauri\target\release\windows-c-drive-cleanup-advisor.exe`
- `dist\windows-c-drive-cleanup-advisor-0.1.0-windows-x64.zip`
- `dist\checksums.txt`

Validate the portable package structure and checksums:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\Test-PortablePackage.ps1
```

## How To Verify The Release

```powershell
Get-Content .\dist\checksums.txt
Get-FileHash .\dist\windows-c-drive-cleanup-advisor-0.1.0-windows-x64.zip -Algorithm SHA256
Get-FileHash .\src-tauri\target\release\windows-c-drive-cleanup-advisor.exe -Algorithm SHA256
```

Compare the results with the checksums published in the GitHub Release.
