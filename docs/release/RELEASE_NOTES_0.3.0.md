# Release Notes: Windows C Drive Cleanup Advisor 0.3.0

Status: release candidate; publication date pending.

## Summary

Version 0.3.0 keeps scanning and reporting read-only. It adds a guarded, optional action to move eligible low-risk cache candidates to the Windows Recycle Bin after showing a plan and receiving explicit confirmation. This action is experimental.

## Changes Since 0.2.0

- A native Rust scan kernel performs the default drive walk; the bundled PowerShell scanner remains an explicit fallback via `WCDCA_SCAN_KERNEL=powershell`.
- Local report history, two-report comparison, recommendation filters, and a report-data treemap help review results.
- Scan task-state and watchdog handling, report retention, structured progress, and CI checks have been strengthened.
- Low-risk cache cleanup accepts stored `reportId` and `candidateIds` only. The backend rechecks category and path safety during planning and execution, enforces a conservative Recycle Bin budget, and writes a local JSONL action log.

## Safety and Limitations

- Scan and report generation do not delete, move, upload, or change files outside their local report output.
- Cleanup is limited to recommendations marked both `low-risk-cache` and `cleanable`. System-managed paths, chat stores, user data, installed applications, reparse points, and unrecognized paths are rejected.
- Cleanup uses the Windows Recycle Bin and has no permanent-delete fallback or elevation helper. A failed or unavailable Recycle Bin blocks the action.
- A completed cleanup makes the old report stale. Scan again before using its sizes or candidates.
- The app and portable executable are unsigned. Testing has been concentrated on one Windows 11 machine; Windows 10, other locales, and bulk or full-Recycle-Bin behavior still need broader validation.

## Distribution

The supported artifact is `windows-c-drive-cleanup-advisor-0.3.0-windows-x64.zip` with `checksums.txt`. Extract the complete archive before launching the executable; it needs the adjacent `_up_` scanner resource. The standalone executable is not a distributable asset.

Release evidence and outstanding manual checks are recorded in `RELEASE_CHECKLIST_0.3.0.md`.
