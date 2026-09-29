# Release Checklist 0.3.0

Status: release candidate. Keep unchecked items open until their evidence is recorded; do not publish this package as a completed release before the required gates pass.

## Safety and Scope

- [x] Scan and report remain read-only, with local-only output (safety and scanner contract checks).
- [x] Cleanup accepts `reportId + candidateIds` only and uses the stored report to resolve paths (Rust and frontend tests).
- [x] Only `low-risk-cache && cleanable` candidates can be selected and planned; blocked categories are rechecked at execution (Rust and frontend tests).
- [x] Planning shows accepted items, rejections, and Recycle Bin budget before confirmation (frontend tests).
- [x] Execution uses the Recycle Bin with no permanent-delete fallback and records per-item outcomes in the action log (Rust tests and real one-file smoke).
- [x] No elevation helper, generic shell permission, or network upload is present (safety boundary check).

## Automated Verification

- [x] A clean `npm ci` followed by `npm run verify` passed all 15 steps, including audit, frontend coverage, Rust tests, release build, packaging, checksum, and extracted scanner runtime smoke. Evidence below.
- [x] `WCDCA_RECYCLE_SMOKE=1 cargo test recycle_smoke` passed using a temporary test file; the Recycle Bin Shell view gained one `smoke-file.bin` item (0 before, 1 after).
- [x] `dist/checksums.txt` verifies the executable and portable zip through `Test-PortablePackage.ps1`.
- [x] The portable zip contains the executable, `_up_` scanner resource, README, user guide, `AGENT.md`, license, this checklist, release notes, and smoke-test template (`Test-PortablePackage.ps1`).

## Independent Portable Runtime and UI

- [x] Extracted the zip to a fresh temporary directory and launched its executable without a development server; after 6 seconds it was still running with a nonzero main-window handle and title `Windows C 盘清理顾问`, then closed normally.
- [ ] Confirm the Chinese UI, safety text, Quick scan default, and report/history panels.
- [ ] Start and cancel a scan; then complete a scan, open both report formats, and confirm `privacy.uploaded == false`.
- [ ] Restart the extracted app and reload the latest local report.
- [ ] Confirm eligible cache selection, plan contents, rejected-item reasons, and action-log display against the real backend. Only use controlled disposable test data for execution.
- [ ] Confirm system-managed and non-cache items cannot enter the cleanup plan or execute path.

## Release Decision

- [ ] Record tested Windows version and locale, remaining untested environments, checksum values, and any limitations in the smoke-test record.
- [ ] Publish only the portable zip and checksum assets after the evidence above is complete. Preserve the existing `v0.2.0` release assets.

## Evidence

Local checks on 2026-09-30 (Asia/Shanghai), Windows 11 Home China 10.0.26200 x64, locale 0804:

- `npm ci` → exit 0, 262 packages installed, 263 audited, 0 vulnerabilities.
- `npm run verify` → exit 0. Version contract 0.3.0; safety and scanner contract passed; seed-tree native 0.03s / PowerShell 2.73s / 92.5x; lint and formatting passed; frontend 45/45 tests, coverage 85% statements / 79.18% branches / 89.37% functions / 85.43% lines; frontend build passed; `npm audit` 0 vulnerabilities; Rust 84/84 tests and `cargo check` passed; Tauri release build, portable package, checksum, and extracted scanner runtime validation passed.
- `WCDCA_RECYCLE_SMOKE=1 cargo test recycle_smoke -- --nocapture` → exit 0, 1/1 selected test passed; Recycle Bin Shell view count for `smoke-file.bin` increased from 0 to 1.
- Independent extracted executable launch → process remained open with main-window handle 1575672 and Chinese title after 6 seconds, then its main window accepted a close request. This verifies startup, not the full GUI workflow.

The real-backend cleanup UI flow and the remaining manual checks above are pending. Windows 10, non-Chinese locales, bulk/full-Recycle-Bin behavior, and non-NTFS volumes remain untested. Exact artifact hashes live in `dist/checksums.txt` to avoid making the packaged checklist self-referential.
