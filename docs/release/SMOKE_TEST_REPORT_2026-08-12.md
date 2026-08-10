# GUI Smoke Test Report: v0.2.0

> Evidence captured on 2026-08-10 for the planned 2026-08-12 release window.

## Build Under Test

- Version: `0.2.0`
- Tester: Codex-assisted GUI smoke test
- Release executable: `src-tauri\target\release\windows-c-drive-cleanup-advisor.exe`
- Portable archive: `dist\windows-c-drive-cleanup-advisor-0.2.0-windows-x64.zip`
- Portable archive SHA-256: `CF828F3A8883F53B257890A8D5D66A04D8B42B3B2C33B3D7B3BD30BD2BC7E9A2`

## Automated Gate

- [x] Clean dependency install completed with `npm ci`.
- [x] `npm run verify` passed: version contract, safety boundary, scanner contract, 24 frontend tests, zero moderate-or-higher audit findings, 29 Rust tests, Rust check, Tauri release build, portable package, checksums, package-content check, and extracted portable runtime scan.
- [x] The final bundle contains the executable, `_up_` scanner resources, MIT `LICENSE`, README, user guide, handoff document, release notes, and release checklist.

## Release GUI Check

- [x] The final release executable starts with title `Windows C 盘清理顾问` and a Chinese UI.
- [x] Quick scan is the default; Deep scan can be selected and presents its deeper-read-only description.
- [x] The safety ledger visibly states zero cleanup actions, zero uploads, reparse-point skipping, blocked system items, and protected-path notes.
- [x] No cleanup, delete, move, uninstall, upload, telemetry, generic shell, or write-action control was visible.
- [x] WebView console after interaction: 0 errors and 0 warnings.

## Start and Cancel

- [x] Scan `da521737-43d4-4985-b863-b7b710cab265` started in Quick mode, showed progress, stage text, elapsed time, current directory, and activity feed.
- [x] Cancelling returned the UI to `扫描已取消。本次未执行任何清理动作。` and re-enabled the mode and start controls.
- [x] A second Quick scan (`e8662c87-63ff-457d-9f21-d17ff4c1b82b`) remained at 27% while enumerating the real local `C:\common_attachment` directory for 4 minutes 48 seconds. The companion panel continued updating elapsed/stage time and retained a usable cancel control; cancelling again completed immediately with the same no-cleanup confirmation.

This workstation-specific large directory did not complete within the observation window. It is not recorded as a completed new scan. The bundled runtime scan and the existing completed local report below provide the completion-path evidence.

## Completed Local Report Recheck

- [x] Restarted final executable and loaded local report `bccae66f-1bb5-4464-b5d5-59151548973b` without starting a scan.
- [x] The report shows 80 recommendations, 200 protected/unreadable paths, 226 skipped reparse points, and 8 blocked system-managed items.
- [x] Report health states that the report is local-only; its JSON has `privacy.uploaded: false`.
- [x] Cache candidates are presented as manual-review-only low-risk guidance, and Windows-managed paths are presented as blocked guidance.
- [x] `复制报告路径` confirmed `报告路径已复制。`.
- [x] `显示 Markdown`, `显示 JSON`, and `打开报告目录` all opened their local report targets.

## Decision

- [x] Pass for v0.2.0 portable release: all automated release gates and the final GUI safety, cancellation, report-loading, report-opening, privacy, and blocked-path checks passed.
- [x] The long-directory observation is documented for future performance work; it is not a safety or data-loss issue and the user can cancel it.
