# Release Checklist: v0.2.0

Use this checklist before publishing the public Windows x64 portable release.

## Scope and Safety

- [ ] The version contract passes: Tauri, npm, and Cargo are all `0.2.0`.
- [ ] The app remains advisory only: no deletion, move, uninstall, upload, telemetry, or settings change.
- [ ] No generic Tauri shell plugin or shell capability is enabled.
- [ ] Rust launches only the bundled scanner with fixed PowerShell arguments.
- [ ] Low-risk cache candidates remain `cleanable=false`.
- [ ] System-managed paths remain blocked guidance.

## Automated Release Gate

- [ ] Run `npm ci` from a clean dependency state.
- [ ] Run `npm run verify` successfully.
- [ ] `npm audit --audit-level=moderate` reports zero vulnerabilities.
- [ ] Portable runtime smoke confirms the extracted scanner emits heartbeats, writes Markdown/JSON, and does not change its source file.
- [ ] `dist\checksums.txt` contains both the portable zip and the release executable hashes.

## Portable Package Contents

- [ ] `windows-c-drive-cleanup-advisor.exe`
- [ ] `_up_\scripts\Scan-CDriveCleanupAdvisor.ps1`
- [ ] `README.md`, `USER-GUIDE.md`, `AGENT.md`, and `LICENSE`
- [ ] `docs\README.md`
- [ ] `docs\release\RELEASE_CHECKLIST_0.2.0.md`
- [ ] `docs\release\RELEASE_NOTES_0.2.0.md`
- [ ] `docs\release\SMOKE_TEST_REPORT_TEMPLATE.md`
- [ ] `docs\product\PRODUCT_PLAN.md`

## Manual GUI Smoke

- [ ] Launch the extracted release executable on Windows 10 or 11 x64.
- [ ] Confirm Chinese UI, visible safety ledger, and Quick scan as the default.
- [ ] Start and cancel one scan; confirm no cleanup action and no stale status overwrite.
- [ ] Complete a Quick scan; inspect the report health panel and cache-directory aggregation.
- [ ] Change to Deep scan and confirm the mode description and read-only behavior.
- [ ] Open Markdown, JSON, and report folder; copy report paths.
- [ ] Restart and load the latest local report.
- [ ] Confirm `privacy.uploaded` is `false`, protected paths remain grouped notes, and system-managed items remain blocked.
- [ ] Record the result in `SMOKE_TEST_REPORT_2026-08-12.md`.

## Publication

- [ ] GitHub repository is public and uses the MIT license.
- [ ] Push `main`, `v0.1.0`, and annotated `v0.2.0` tags.
- [ ] Create a Draft Release with only the portable zip and `checksums.txt` attached.
- [ ] Download the Draft asset into a new temporary directory, validate its SHA-256, extract it, and launch it.
- [ ] Release text states Windows 10/11 x64, local-only reports, read-only scope, no telemetry, and unsigned SmartScreen caveat.
- [ ] Publish only after the downloaded Draft passes its verification.
