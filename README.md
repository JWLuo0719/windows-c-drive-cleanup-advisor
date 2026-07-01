# Windows C Drive Cleanup Advisor

A read-only Windows desktop advisor for diagnosing C drive disk pressure. It scans real local paths, skips reparse-point locations, ranks large folders and files, and explains what is safe to review.

## Current Status

Version `0.1.0` is intentionally advisory only:

- Tauri v2 desktop shell with React/Vite UI.
- Rust owns the narrow IPC boundary.
- The bundled PowerShell scanner runs with fixed arguments from Rust.
- Markdown and enriched JSON reports are written locally.
- Report actions can reveal Markdown/JSON output, open the report folder, or copy report paths.
- The latest local report can be loaded again after restarting the app.
- The result health panel checks privacy status, recommendation count, blocked system-managed items, unreadable paths, and skipped reparse points.
- The scan companion panel shows elapsed time, current-stage time, stage explanations, stage reason and next-step cues, rotating tips, and a recent activity feed while scans are running. Long top-root and large-file stages emit low-frequency heartbeat updates so the app feels active even when a scan stage takes several minutes.
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


## Scan Modes

The desktop app offers two read-only scan modes:

- **Quick scan**: default mode. It scans real top-level C drive usage and large files, skips reparse points, and avoids extra duplicate drilldowns so the result is usable in a few minutes on typical machines.
- **Deep scan**: includes the common-root drilldowns for more detail. Use it when you can wait longer and want a more granular report.

Both modes write local Markdown and JSON reports only. Neither mode deletes, moves, uploads, uninstalls, or changes settings.

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

Before publishing a GitHub Release, walk through [docs/release/RELEASE_CHECKLIST.md](docs/release/RELEASE_CHECKLIST.md).
Use [docs/release/SMOKE_TEST_REPORT_TEMPLATE.md](docs/release/SMOKE_TEST_REPORT_TEMPLATE.md) to record the manual GUI smoke test for the release executable.


## Reading Results

The app separates review items into conservative risk groups. Low-risk cache items are still manual-review only in v0.1. System-managed items are blocked and should be handled only through Windows or vendor tools. Permission-denied notes are expected for protected folders and do not mean the scan failed.

After a scan completes, the result health panel gives a quick sanity check for whether the report is suitable to review: it confirms local-only privacy status, checks that recommendations were generated, verifies system-managed items remain blocked, summarizes unreadable paths, and reports skipped reparse points.

The UI prioritizes more specific review candidates ahead of broad root-folder summaries, so low-risk cache and app-managed findings are easier to inspect first. Unreadable paths are grouped by protected area, such as Defender, Windows system folders, recycle bin identities, or Microsoft Store app folders.

Use **载入最近报告** in the local report panel to reopen the latest local JSON report without running another scan.

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
- `v0.2`: Manual smoke polish, scan waiting experience, signed release planning, and installer packaging.
- `v0.3`: Experimental low-risk cache cleanup allowlist, using `reportId + candidateIds` only and moving items to the recycle bin by default.
- `v1.0`: Signed or clearly unsigned release, installer, portable zip, checksums, complete release verification guide.

See [docs/product/PRODUCT_PLAN.md](docs/product/PRODUCT_PLAN.md) for the scan waiting experience plan. A companion panel, recent activity feed, scanner heartbeat updates, and stage reason/next-step cues are implemented; richer interaction and optional pet-style reactions remain future v0.2 polish.

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

That validation also extracts the portable zip and runs the bundled scanner script against a temporary drive, confirming release resources can emit progress heartbeats and write local Markdown/JSON reports.

## How To Verify The Release

```powershell
Get-Content .\dist\checksums.txt
Get-FileHash .\dist\windows-c-drive-cleanup-advisor-0.1.0-windows-x64.zip -Algorithm SHA256
Get-FileHash .\src-tauri\target\release\windows-c-drive-cleanup-advisor.exe -Algorithm SHA256
```

Compare the results with the checksums published in the GitHub Release.
