# GUI Smoke Test Report: 2026-06-30

## Build Under Test

- Version: 0.1.0
- Date: 2026-06-30
- Tester: Codex assisted GUI smoke test
- Release executable path: `D:\Project\windows-c-drive-cleanup-advisor\src-tauri\target\release\windows-c-drive-cleanup-advisor.exe`
- Portable zip path: `D:\Project\windows-c-drive-cleanup-advisor\dist\windows-c-drive-cleanup-advisor-0.1.0-windows-x64.zip`
- SHA-256 checked against `dist\checksums.txt`: yes

## Automated Preflight

- [x] `npm run verify` passed before GUI smoke testing.
- [x] Release executable launches without a white screen.
- [x] Window title is `Windows C 盘清理顾问`.
- [x] UI is Chinese.
- [x] Safety ledger is visible.
- [x] Quick scan is selected by default.
- [x] No generic shell prompt, cleanup action, delete action, upload action, or telemetry prompt was visible.

## Quick Scan Waiting Experience

- Scan ID: `9270bcd6-3d37-44d5-8578-b934dffb7aae`
- Start time: 2026-06-30 22:19:49 local time
- End time: 2026-06-30 22:27:59 local time
- Approximate duration: 8 minutes 10 seconds

Checklist:

- [x] Progress starts and status messages update.
- [x] Scan companion panel appears while scanning.
- [x] Companion panel shows elapsed time.
- [x] Companion panel shows current-stage time.
- [x] Companion panel shows current focus.
- [x] Companion panel explains why the current stage may be slow.
- [x] Companion panel shows the likely next step.
- [x] Activity feed records recent scan messages.
- [x] During top-root sizing, heartbeat messages appear instead of a blank/frozen UI.
- [x] During long single-directory sizing, activity feed now adds an explanatory idle-stage activity message after the follow-up fix.
- [ ] Large-file enumeration was not visually observed in this run because the scan jumped from top-root sizing to completion between polling snapshots.
- [x] Cancel button remains available while scanning.

Notes:

```text
The scan stayed around 27% at C:\common_attachment for roughly 8 minutes.
The companion timer, current-stage timer, current focus, reason, and next-step cues kept updating.
However, the activity feed did not receive fresh backend heartbeats while that single directory was being counted.
Follow-up implemented after this run: frontend idle-stage activity fallback adds explanatory activity messages when no backend heartbeat arrives for a long stage.
Short retest on rebuilt release exe confirmed the activity feed shows:
"当前阶段仍在工作：正在统计 C 盘顶层真实目录，这一步遇到大目录时会停留较久。"
```

## Follow-up Short Retest

- Scan ID: `546dd90f-1275-481c-bf32-ac5763c8da22`
- Test type: start Quick scan, wait 40+ seconds, confirm idle-stage activity fallback, then cancel.

Checklist:

- [x] Rebuilt release executable launches.
- [x] Quick scan starts.
- [x] Long unchanged stage shows explanatory idle-stage activity.
- [x] Real progress percent stays unchanged while the explanatory activity is added.
- [x] Cancel action returns the UI to `扫描已取消。本次未执行任何清理动作。`

## Completed Report Path

- Scan ID: `9270bcd6-3d37-44d5-8578-b934dffb7aae`
- Markdown report path: `C:\Users\34590\AppData\Roaming\com.local.windows-c-drive-cleanup-advisor\reports\9270bcd6-3d37-44d5-8578-b934dffb7aae\c-drive-cleanup-advisor-20260630-221949.md`
- JSON report path: `C:\Users\34590\AppData\Roaming\com.local.windows-c-drive-cleanup-advisor\reports\9270bcd6-3d37-44d5-8578-b934dffb7aae\scan-report-9270bcd6-3d37-44d5-8578-b934dffb7aae.json`

Checklist:

- [x] Completed scan loads results automatically.
- [x] Result summary reports 80 recommendations.
- [x] JSON has `privacy.uploaded` set to `false`.
- [x] Result guide is visible.
- [x] Result health panel reports local-only privacy status.
- [x] Result health panel summarizes recommendation count.
- [x] Result health panel reports unreadable paths: 200.
- [x] Result health panel reports skipped reparse points: 226.
- [x] Unreadable paths are grouped by protected area instead of shown as a long raw list.
- [x] Low-risk cache guidance remains manual review only.
- [x] System-managed or protected Windows paths are marked as blocked guidance, not cleanup tasks.
- [x] Latest local report can be loaded after relaunch without starting a new scan.
- [x] Copy-report-paths action shows `报告路径已复制。`
- [x] Markdown/JSON reveal buttons open Explorer on the expected report folder.
- [x] Backend report generation now prioritizes specific review candidates ahead of broad root-folder summaries before truncating the enhanced recommendation list.

## Markdown/JSON Reveal Retest

- Date: 2026-07-01
- Test type: release executable GUI click-through using WebView2 CDP against the real Tauri window.
- Loaded report: `9270bcd6-3d37-44d5-8578-b934dffb7aae`
- Report folder: `C:\Users\34590\AppData\Roaming\com.local.windows-c-drive-cleanup-advisor\reports\9270bcd6-3d37-44d5-8578-b934dffb7aae`

Checklist:

- [x] Release executable exposes the real `Windows C 盘清理顾问` WebView.
- [x] `载入最近报告` is enabled before a report is loaded.
- [x] `显示 Markdown` and `显示 JSON` are disabled before loading a report.
- [x] Clicking `载入最近报告` loads the latest local report without starting a scan.
- [x] `显示 Markdown` becomes enabled after the latest report is loaded.
- [x] `显示 JSON` becomes enabled after the latest report is loaded.
- [x] Clicking `显示 Markdown` opens Explorer on the expected report folder.
- [x] Clicking `显示 JSON` opens Explorer on the expected report folder.
- [x] The loaded report links match the Markdown and JSON files recorded above.

Evidence:

```text
REPORT_FOLDER=C:\Users\34590\AppData\Roaming\com.local.windows-c-drive-cleanup-advisor\reports\9270bcd6-3d37-44d5-8578-b934dffb7aae
EXPLORER_BEFORE=0
loaded button: 载入最近报告
markdown button: 显示 Markdown
json button: 显示 JSON
EXPLORER_AFTER=2
```

## Final Manual Smoke Pass

- Date: 2026-07-01
- Tester: User-guided manual release smoke test
- Test type: launch release executable, review safety boundary, cancel scan, run completed Quick scan, check report actions, restart/load latest report, and inspect JSON privacy flag.
- Completed scan ID: `bccae66f-1bb5-4464-b5d5-59151548973b`
- Markdown report path: `C:\Users\34590\AppData\Roaming\com.local.windows-c-drive-cleanup-advisor\reports\bccae66f-1bb5-4464-b5d5-59151548973b\c-drive-cleanup-advisor-20260701-134007.md`
- JSON report path: `C:\Users\34590\AppData\Roaming\com.local.windows-c-drive-cleanup-advisor\reports\bccae66f-1bb5-4464-b5d5-59151548973b\scan-report-bccae66f-1bb5-4464-b5d5-59151548973b.json`

Checklist:

- [x] User completed the requested manual smoke flow before v0.1 packaging.
- [x] Launch and Chinese UI review passed.
- [x] Safety boundary review passed; no cleanup, delete, upload, or system-change action was reported.
- [x] Cancel scan path passed.
- [x] Completed Quick scan path passed.
- [x] Markdown, JSON, report folder, and copy-path actions were tested without reported issues.
- [x] Restart/load-latest-report path passed.
- [x] Deep scan mode entry was reviewed without reported issues.
- [x] Latest JSON report has `privacy.uploaded` set to `false`.
- [x] Latest JSON report generated 80 recommendations.
- [x] Latest JSON report recorded 200 unreadable paths and 226 skipped reparse points, matching expected normal-user scan behavior.

JSON privacy evidence:

```text
scan-report-bccae66f-1bb5-4464-b5d5-59151548973b.json:210  "privacy": {
scan-report-bccae66f-1bb5-4464-b5d5-59151548973b.json:211    "uploaded": false
```

## Decision

- [x] Pass: release is ready for the next packaging/publication step.
- [ ] Hold: Markdown/JSON reveal actions still need direct GUI click-through verification.

Blocking issues:

```text
No blocking safety issue found. The app stayed read-only, completed the full scan, and the follow-up cancellation path stayed read-only.
```

Follow-up polish:

```text
No v0.1-blocking GUI smoke item remains. Keep result-review aggregation and waiting-experience polish for v0.2.
```
