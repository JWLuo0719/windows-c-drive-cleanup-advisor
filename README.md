# Windows C Drive Cleanup Advisor

A Windows desktop advisor for diagnosing C drive disk pressure. Scanning and reporting are read-only: it scans real local paths, skips reparse-point locations, ranks large folders and files, and explains what is safe to review. The source tree additionally carries an experimental, plan-first recycle-bin cleanup for low-risk cache candidates (see *Experimental Cleanup*).

## Current Status

Published `v0.2.0` is advisory only. The current `v0.3.0` source adds the guarded experimental cleanup described below; scanning and reporting remain read-only:

- Tauri v2 desktop shell with React/Vite UI.
- Rust owns the narrow IPC boundary.
- An in-process Rust kernel scans by default; Rust can launch the bundled PowerShell scanner with fixed arguments as an explicit fallback.
- Markdown and enriched JSON reports are written locally.
- Report actions can reveal Markdown/JSON output, open the report folder, or copy report paths.
- The latest local report can be loaded again after restarting the app.
- Repeated low-risk cache files under the same review directory are combined into one directory-level candidate, so a cache does not flood the recommendation list with file rows.
- The result health panel checks privacy status, recommendation count, blocked system-managed items, unreadable paths, and skipped reparse points.
- The scan companion panel shows elapsed time, current-stage time, stage explanations, stage reason and next-step cues, rotating tips, and a recent activity feed while scans are running. Long top-root and large-file stages emit low-frequency heartbeat updates so the app feels active even when a scan stage takes several minutes.
- Scans never clean up: no cleanup, deletion, move, uninstall, upload, or settings change is performed during scanning or report generation.

## Experimental Cleanup (source tree)

Beyond the read-only scan, the current source tree offers a guarded, opt-in cleanup path. This feature is not part of the published `v0.2.0` portable package:

- A checkbox appears only on cleanable **low-risk-cache** recommendations (npm/pip/temp-style caches); system-managed and user-data items can never be selected.
- Cleanup is **plan-first**: *计划清理* calls the backend for a dry-run plan (validated items, rejected items with reasons, recycle budget) before anything is touched; only *确认移入回收站* executes.
- Execution re-validates every candidate server-side (allowlist, system-managed re-check, symlink/reparse rejection, file-identity recheck), moves items to the **recycle bin only** (never a permanent delete), and appends each outcome to a JSONL audit log shown in the history panel.
- After a cleanup the report data is marked stale; the UI recommends a fresh scan. Run with default user privileges — there is no elevation path.

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

The full verification script runs the safety boundary check (including the Phase 5 deletion-surface assertions: the delete executor is confined to `cleanup.rs`, the rest of the product code stays read-only, and no elevation or network APIs exist), scanner output contract check, frontend tests, frontend build, npm audit, Rust tests, Rust check, Tauri release build, portable packaging, checksum generation, and package validation.
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

GitHub Actions workflow `.github/workflows/ci.yml` runs on Windows with two jobs: a fast job (pull requests and pushes) that runs the safety, contract, test, build, audit, and Rust gates via `Invoke-ProjectChecks.ps1 -SkipTauriBuild -SkipPackage -SkipChecksum`, and a full job (`main` pushes and manual dispatch) that runs `npm run verify` including the Tauri release build, portable packaging, checksum generation, `sha256sum -c` validation, and package validation, then uploads the artifacts. Both jobs use `Swatinem/rust-cache` and pin `permissions: contents: read`. Launching the portable exe against its bundled resources remains a local release gate with recorded runtime evidence.

Before publishing the next GitHub Release, walk through [docs/release/RELEASE_CHECKLIST_0.3.0.md](docs/release/RELEASE_CHECKLIST_0.3.0.md).
Use [docs/release/SMOKE_TEST_REPORT_TEMPLATE.md](docs/release/SMOKE_TEST_REPORT_TEMPLATE.md) to record the manual GUI smoke test for the release executable.


## Reading Results

The app separates review items into conservative risk groups. Low-risk cache items are still manual-review only in v0.2. System-managed items are blocked and should be handled only through Windows or vendor tools. Permission-denied notes are expected for protected folders and do not mean the scan failed.

After a scan completes, the result health panel gives a quick sanity check for whether the report is suitable to review: it confirms local-only privacy status, checks that recommendations were generated, verifies system-managed items remain blocked, summarizes unreadable paths, and reports skipped reparse points.

The UI prioritizes more specific review candidates ahead of broad root-folder summaries, so low-risk cache and app-managed findings are easier to inspect first. Unreadable paths are grouped by protected area, such as Defender, Windows system folders, recycle bin identities, or Microsoft Store app folders.

Use **载入最近报告** in the local report panel to reopen the latest local JSON report without running another scan.

## Safety Boundary

The app is built around a conservative rule: diagnose first, let the user decide. The frontend does not receive generic shell permissions and cannot invoke arbitrary commands. Rust exposes fixed scan, report, and guarded cleanup commands.

`scripts\Test-SafetyBoundary.ps1` is part of local and CI verification. It checks that the bundled scanner remains read-only, that no Tauri shell permission is granted to the frontend, and that Rust still owns scanner process launch with fixed arguments.

## What This App Never Deletes

Scanning never deletes anything — a scan only reads and writes its local report. The experimental cleanup, if you use it, moves only allowlisted low-risk cache candidates to the recycle bin (preview first, every attempt logged) and never applies to any of the following, which are also never handled automatically:

- `C:\Windows\WinSxS`
- `C:\Windows\Installer`
- `C:\Windows\System32`
- `C:\Windows\servicing`
- `C:\System Volume Information`
- `C:\Recovery`
- `C:\$Recycle.Bin`
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
- `v0.2`: Directory-level cache aggregation, scan waiting feedback, public portable release, checksums, and MIT licensing.
- `v0.3`: Experimental low-risk cache cleanup allowlist, using `reportId + candidateIds` only and moving items to the recycle bin by default.
- `v1.0`: Optional installer, code signing, and a separately reviewed cleanup capability design.

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
- `dist\windows-c-drive-cleanup-advisor-0.3.0-windows-x64.zip`
- `dist\checksums.txt`

Validate the portable package structure and checksums:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\Test-PortablePackage.ps1
```

That validation also extracts the portable zip and runs the bundled scanner script against a temporary drive, confirming release resources can emit progress heartbeats and write local Markdown/JSON reports.

## How To Verify The Release

```powershell
Get-Content .\dist\checksums.txt
Get-FileHash .\dist\windows-c-drive-cleanup-advisor-0.3.0-windows-x64.zip -Algorithm SHA256
Get-FileHash .\src-tauri\target\release\windows-c-drive-cleanup-advisor.exe -Algorithm SHA256
```

Compare the results with the checksums published in the GitHub Release.
