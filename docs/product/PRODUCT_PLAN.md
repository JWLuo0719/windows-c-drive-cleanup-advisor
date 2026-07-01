# Product Plan

This document tracks user-facing improvements that are not required for the read-only safety boundary, but matter for everyday usability.

## Scan Waiting Experience

User feedback from a Quick scan: progress stayed at 18% for a long time, jumped to 65%, then stayed at 65% for another long stretch. If scan time cannot be reduced much further, the app should make the waiting period feel clearer and less boring.

### Goals

- Keep the scanner read-only and avoid adding risky permissions.
- Make long scan stages feel active even when the numeric percentage is not moving.
- Explain what the app is doing in plain Chinese during each stage.
- Reduce user boredom during 5-10 minute scans without distracting from the safety-first purpose.

### Candidate Ideas

- Implemented first pass: show elapsed time, current-stage time, stage explanations, and rotating safety tips while scanning.
- Implemented activity feed: keep the latest scan messages visible while scanning, so users can see recent stage changes.
- Implemented scanner heartbeat events: long top-root sizing, common-root drilldown, and large-file enumeration now emit low-frequency progress updates that keep the activity feed moving during the former 18% and 65% stalls.
- Implemented richer stage feedback: the companion panel now shows the current focus, why the stage can be slow, and the likely next step.
- Implemented idle-stage activity fallback: if the backend has no new heartbeat for a long stage, the activity feed adds a read-only "current stage is still working" update without changing the real progress percent.
- Expand the activity feed with richer scanner events, for example "正在枚举大文件候选", "正在跳过重解析点", "正在整理报告".
- Add a small interactive companion/pet that reacts to scan events, idle time, warnings, and completion.
- Add rotating safety and interpretation tips during long unchanged progress periods.
- Add a "what has been found so far" preview if the backend can expose partial summaries without slowing the scan.

### Acceptance Criteria

- First pass implemented: during a long stage, the companion panel updates timers and rotating tips.
- First pass implemented: progress stalls are explained as "current stage is still running", not as a frozen app.
- Activity feed implemented: recent scan messages remain visible while the scan is running.
- Scanner heartbeat implemented: long-running scan loops emit real activity updates without changing the read-only report contract.
- Stage feedback implemented: stalled stages show a reason and next-step cue in the companion panel.
- Idle fallback implemented: a long unchanged stage still produces periodic explanatory activity messages without pretending progress advanced.
- The user can still cancel the scan at any time.
- The feature does not change scanner output, delete files, upload data, or require generic shell access.
- Tests cover stalled-progress messaging and scan event interpretation logic.

### Priority

Target: v0.2 usability polish, after the current v0.1 read-only release path is stable.

## Result Review Experience

User feedback and scan `4544e04d-694c-4aea-bd3b-6b334da83462` showed that broad root-folder summaries and many similar candidates can make the result list feel harder to act on.

### Implemented

- Enhanced report generation now sorts recommendations by review usefulness before truncating: low-risk cache, app-managed data, user-data, uninstall/migration, then blocked system-managed guidance.
- Broad root-folder summaries such as `C:\Users` and `C:\Program Files` are kept behind specific paths in their category, so the first screen is more likely to show concrete review targets.
- The UI can load the latest local report after restart and copy report paths without rerunning a long scan.
- Repeated low-risk cache files under the same cache directory, for example NVIDIA `DXCache`, are aggregated into one folder-level candidate when the report does not already contain that directory.

### Next Candidates

- Add a "why this is first" hint for top recommendations.
- Add a result-level "best next 3 checks" panel once the report has enough signal.
