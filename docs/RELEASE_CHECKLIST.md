# Release Checklist

Use this checklist before publishing a public Windows release.

## v0.1 Scope

- [ ] The app is advisory only.
- [ ] No cleanup, deletion, move, uninstall, upload, telemetry, or settings change is performed.
- [ ] The frontend only invokes fixed Tauri commands.
- [ ] No generic Tauri shell plugin or shell permission is enabled.
- [ ] The bundled scanner is launched by Rust with fixed PowerShell arguments.
- [ ] Markdown and JSON reports are written locally.

## Required Local Verification

Run:

```powershell
npm run verify
```

This must pass all gates:

- [ ] Read-only safety boundary check.
- [ ] Scanner output contract check.
- [ ] Frontend tests.
- [ ] Frontend production build.
- [ ] `npm audit --audit-level=moderate`.
- [ ] Rust unit tests.
- [ ] Rust check.
- [ ] Tauri release build.
- [ ] Portable package creation.
- [ ] SHA-256 checksum generation.
- [ ] Portable package validation.

## Release Artifacts

Confirm these files exist after `npm run verify`:

- [ ] `src-tauri\target\release\windows-c-drive-cleanup-advisor.exe`
- [ ] `dist\windows-c-drive-cleanup-advisor-0.1.0-windows-x64.zip`
- [ ] `dist\checksums.txt`

Confirm the portable zip contains:

- [ ] `windows-c-drive-cleanup-advisor.exe`
- [ ] bundled Tauri resources under `_up_`
- [ ] `scripts\Scan-CDriveCleanupAdvisor.ps1`
- [ ] `README.md`
- [ ] `USER-GUIDE.md`
- [ ] `docs\RELEASE_CHECKLIST.md`

## Manual Smoke Test

- [ ] Launch the release executable on Windows.
- [ ] Confirm the UI is Chinese and shows the safety ledger.
- [ ] Start a scan as a normal user.
- [ ] Confirm progress updates are visible.
- [ ] Cancel a running scan and confirm no cleanup action occurs.
- [ ] Run one full scan and confirm Markdown/JSON reports open from the UI.
- [ ] Confirm `privacy.uploaded` is `false` in the JSON report.
- [ ] Confirm scan notes mention skipped or unreadable paths when applicable.

## Release Notes

Include:

- [ ] Version number and date.
- [ ] Read-only status.
- [ ] Safety boundary summary.
- [ ] Privacy statement.
- [ ] Known false positives.
- [ ] SmartScreen notice if unsigned.
- [ ] SHA-256 checksums from `dist\checksums.txt`.
- [ ] How to verify the downloaded files.

## Do Not Release If

- [ ] Any verification gate fails.
- [ ] The scanner contains destructive commands.
- [ ] Tauri shell permissions are enabled for the frontend.
- [ ] Reports are uploaded or telemetry is added.
- [ ] Cleanup actions are exposed in the UI before the v0.3 allowlist design is implemented.
