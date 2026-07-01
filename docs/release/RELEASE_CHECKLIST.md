# Release Checklist

Use this checklist before publishing a public Windows release.

## v0.1 Scope

- [x] The app is advisory only.
- [x] No cleanup, deletion, move, uninstall, upload, telemetry, or settings change is performed.
- [x] The frontend only invokes fixed Tauri commands.
- [x] No generic Tauri shell plugin or shell permission is enabled.
- [x] The bundled scanner is launched by Rust with fixed PowerShell arguments.
- [x] Markdown and JSON reports are written locally.

## Required Local Verification

Run:

```powershell
npm run verify
```

This must pass all gates:

- [x] Read-only safety boundary check.
- [x] Scanner output contract check.
- [x] Frontend tests.
- [x] Frontend production build.
- [x] `npm audit --audit-level=moderate`.
- [x] Rust unit tests.
- [x] Rust check.
- [x] Tauri release build.
- [x] Portable package creation.
- [x] SHA-256 checksum generation.
- [x] Portable package validation.
- [x] Portable runtime smoke test: extracted bundled scanner emits heartbeat progress and writes local Markdown/JSON reports.

## Release Artifacts

Confirm these files exist after `npm run verify`:

- [x] `src-tauri\target\release\windows-c-drive-cleanup-advisor.exe`
- [x] `dist\windows-c-drive-cleanup-advisor-0.1.0-windows-x64.zip`
- [x] `dist\checksums.txt`

Confirm the portable zip contains:

- [x] `windows-c-drive-cleanup-advisor.exe`
- [x] bundled Tauri resources under `_up_`
- [x] `scripts\Scan-CDriveCleanupAdvisor.ps1`
- [x] `README.md`
- [x] `USER-GUIDE.md`
- [x] `AGENT.md`
- [x] `docs\release\RELEASE_CHECKLIST.md`
- [x] `docs\product\PRODUCT_PLAN.md`
- [x] `docs\release\RELEASE_NOTES_0.1.0.md`
- [x] `docs\release\SMOKE_TEST_REPORT_TEMPLATE.md`

## Manual Smoke Test

- [x] Create a smoke test report from `docs\release\SMOKE_TEST_REPORT_TEMPLATE.md`.
- [x] Launch the release executable on Windows.
- [x] Confirm the UI is Chinese and shows the safety ledger.
- [x] Confirm Quick scan is selected by default.
- [x] Start a Quick scan as a normal user.
- [x] Confirm progress updates are visible.
- [x] Confirm the scan companion panel shows elapsed time, current-stage time, and stage explanations while scanning.
- [x] Confirm the scan companion panel explains the current focus, why the stage may be slow, and the likely next step.
- [x] Confirm the scan activity feed records recent scan messages while scanning.
- [x] Confirm long top-root and large-file stages keep adding heartbeat activity instead of appearing frozen at 18% or 65%.
- [x] Confirm a long unchanged stage adds a "current stage is still working" activity message without changing the real progress percent.
- [x] Cancel a running scan and confirm no cleanup action occurs.
- [x] Switch to Deep scan and confirm the mode description changes before starting.
- [x] Run one completed scan and confirm Markdown/JSON reports open from the UI.
- [x] Restart the app and confirm the latest local report is restored without running a new scan.
- [x] Copy report paths from the UI and confirm both Markdown and JSON paths are included.
- [x] Confirm the result guide is visible after a report is available.
- [x] Confirm the result health panel reports local-only privacy, recommendation count, unreadable paths, and reparse-point handling.
- [x] Confirm unreadable paths are grouped by protected area instead of shown as a long raw error list.
- [x] Confirm the recommendation list prioritizes specific review candidates ahead of broad root-folder summaries.
- [x] Confirm low-risk cache guidance remains manual review only.
- [x] Confirm system-managed or protected Windows paths are marked as blocked guidance, not cleanup tasks.
- [x] Confirm `privacy.uploaded` is `false` in the JSON report.
- [x] Confirm scan notes mention skipped or unreadable paths when applicable.

## Release Notes

Include:

- [x] Version number and date.
- [x] Read-only status.
- [x] Safety boundary summary.
- [x] Privacy statement.
- [x] Known false positives.
- [x] SmartScreen notice if unsigned.
- [x] SHA-256 checksums from `dist\checksums.txt`.
- [x] How to verify the downloaded files.

## Do Not Release If

- [ ] Any verification gate fails.
- [ ] The scanner contains destructive commands.
- [ ] Tauri shell permissions are enabled for the frontend.
- [ ] Reports are uploaded or telemetry is added.
- [ ] Cleanup actions are exposed in the UI before the v0.3 allowlist design is implemented.
