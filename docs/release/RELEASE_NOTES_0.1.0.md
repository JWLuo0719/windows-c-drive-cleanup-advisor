# Release Notes: Windows C Drive Cleanup Advisor 0.1.0

Release date: 2026-06-30

## Summary

Windows C Drive Cleanup Advisor 0.1.0 is a read-only desktop advisor for diagnosing C drive disk pressure. It scans real local paths, skips reparse-point locations, generates local Markdown and JSON reports, and groups recommendations by review risk.

This release does not delete, move, uninstall, upload, or change settings.

## What Is Included

- Tauri v2 desktop app with Chinese UI.
- Fixed Rust-owned scanner IPC boundary.
- Bundled PowerShell scanner launched with fixed arguments.
- Quick scan and Deep scan modes.
- Local Markdown and enriched JSON reports.
- Result health checks for privacy status, recommendation count, unreadable paths, blocked system-managed items, and skipped reparse points.
- Result guide explaining how to review low-risk cache, system-managed items, protected paths, and reparse points.
- Report actions for Markdown, JSON, report folder, and copying report paths.
- Latest local report loading after app restart.
- Enhanced reports prioritize specific review candidates before broad root-folder summaries.
- Scan companion panel with elapsed time, current-stage time, rotating safety tips, stage reason, next-step cues, and recent scan activity.
- Scanner heartbeat updates for long top-root sizing, common-root drilldown, and large-file enumeration stages.

## Safety Boundary

The app is advisory only. The frontend does not receive generic shell permissions and cannot invoke arbitrary commands. The Rust backend only exposes fixed scan/report commands. The bundled scanner does not contain cleanup actions.

System-managed paths such as `C:\Windows`, `C:\Windows\WinSxS`, `C:\Windows\Installer`, `C:\System Volume Information`, `C:\Recovery`, `C:\$Recycle.Bin`, `pagefile.sys`, `swapfile.sys`, and `hiberfil.sys` are blocked guidance, not cleanup tasks.

## Privacy

Reports are written to local disk only. The app does not upload scan output, file paths, telemetry, or usage data.

## Known Limitations

- Unsigned Windows builds may trigger Microsoft Defender SmartScreen warnings.
- Permission-denied paths are expected for protected folders such as WindowsApps, Defender, Recycle Bin identities, and some ProgramData locations.
- Heuristic categories are advisory. Review paths before taking action outside the app.
- Low-risk cache items are still manual-review only in this release.
- Cleanup actions are not exposed before the future v0.3 allowlist design.

## Verification

Local verification command:

```powershell
npm run verify
```

The release was verified with:

- read-only safety boundary check
- scanner output contract check
- frontend tests
- frontend production build
- `npm audit --audit-level=moderate`
- Rust unit tests
- Rust check
- Tauri release build
- portable package creation
- SHA-256 checksum generation
- portable package validation
- portable runtime smoke test for the extracted bundled scanner

## Artifacts

- `windows-c-drive-cleanup-advisor.exe`
- `windows-c-drive-cleanup-advisor-0.1.0-windows-x64.zip`
- `checksums.txt`
- `docs\release\SMOKE_TEST_REPORT_TEMPLATE.md`

## SHA-256

See `checksums.txt` published next to the release artifacts. The portable zip also contains this release note, so the zip hash is recorded in `checksums.txt` rather than hard-coded here.

To verify downloaded files:

```powershell
Get-FileHash .\windows-c-drive-cleanup-advisor.exe -Algorithm SHA256
Get-FileHash .\windows-c-drive-cleanup-advisor-0.1.0-windows-x64.zip -Algorithm SHA256
```
