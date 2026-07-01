# AGENT.md

Reusable project guidance for agents working on `windows-c-drive-cleanup-advisor`.

Last updated: 2026-07-01

## Current Handoff Snapshot

The project is in `v0.1` ready-to-package state. The main read-only scan/report workflow is implemented and verified. Recent work focused on making real scan results easier to review, making completed reports reusable after app restart, and closing the final Markdown/JSON reveal smoke-test gap.

Latest full verification:

```powershell
npm run verify
```

Last known result: passed after the final user-guided manual smoke pass. It ran safety boundary checks, scanner contract checks, frontend tests, frontend build, npm audit, Rust tests, Rust check, Tauri release build, portable package creation, checksum generation, and portable package validation.

Latest release checksums are in `dist\checksums.txt`. Do not hard-code them in this file because `AGENT.md` is included in the portable zip; embedding the zip hash here would make the package self-referential and stale after every repack.

Known remaining manual smoke item before calling `v0.1` fully ready: none.

Final smoke evidence is recorded in `docs\SMOKE_TEST_REPORT_2026-06-30.md`. The final user-guided manual scan was `bccae66f-1bb5-4464-b5d5-59151548973b`, and its JSON report confirmed `privacy.uploaded` is `false`.

## Project Purpose

This is a read-only Windows C drive cleanup advisor. It diagnoses disk pressure, ranks local space candidates, and explains what the user may review manually.

Current product shape:

- Tauri v2 desktop shell.
- React/Vite/TypeScript frontend.
- Rust owns the narrow IPC boundary.
- Bundled PowerShell scanner writes local Markdown and enriched JSON reports.
- Quick scan is the default; Deep scan adds common-root drilldowns.
- Result guide and result health panel help users judge scan output.
- Scan companion and activity feed keep long scans visibly active.
- Latest local report can be loaded after app restart.
- Enhanced report generation prioritizes specific review candidates before broad root-folder summaries.
- Portable release zip includes executable, bundled scanner resource, README, user guide, `AGENT.md`, release checklist, product plan, release notes, and smoke-test template.

## Non-Negotiable Safety Boundary

Default to advisory-only behavior.

Do not add code that deletes, moves, truncates, uninstalls, uploads, changes settings, disables services, modifies registry state, or silently runs cleanup commands.

Keep these constraints unless the user explicitly requests a scoped design change and the safety model is updated first:

- No generic Tauri shell permission for the frontend.
- No arbitrary command execution from UI input.
- Rust launches the bundled scanner with fixed PowerShell arguments.
- Reports stay local; no telemetry or upload.
- System-managed items remain blocked guidance, not cleanup tasks.
- Low-risk cache items remain manual-review only until a future allowlist design exists.

Treat these as system-managed or high-care areas:

- `C:\Windows\WinSxS`
- `C:\Windows\Installer`
- `C:\Windows\System32`
- `C:\Recovery`
- `C:\System Volume Information`
- `C:\pagefile.sys`, `C:\swapfile.sys`, `C:\hiberfil.sys`
- WSL `ext4.vhdx`
- WeChat, QQ, and WXWork message stores
- installed application directories

## Read First

Before making changes, skim these files:

- `README.md`: current scope, commands, safety boundary, roadmap.
- `USER-GUIDE.md`: user-facing run and result interpretation flow.
- `docs/RELEASE_CHECKLIST.md`: release gates and manual smoke tests.
- `docs/PRODUCT_PLAN.md`: scan waiting experience and result-review planning.
- `docs/RELEASE_NOTES_0.1.0.md`: release-facing feature list and limitations.
- `docs/SMOKE_TEST_REPORT_2026-06-30.md`: latest GUI smoke-test evidence and remaining manual item.
- `SKILL.md`: cleanup-advisor domain heuristics and reporting style.

For code changes, inspect the relevant files:

- `src/App.tsx`: main UI and scan flow.
- `src/reportUtils.ts`: frontend result grouping, summaries, and health checks.
- `src/tauri.ts`: frontend IPC wrappers.
- `src-tauri/src/lib.rs`: Rust commands, scanner launch, report enrichment, safety classification.
- `scripts/Scan-CDriveCleanupAdvisor.ps1`: read-only scanner.
- `scripts/Test-SafetyBoundary.ps1`: safety guardrail checks.
- `scripts/Test-ScannerContract.ps1`: scanner contract check.
- `scripts/Invoke-ProjectChecks.ps1`: local verification pipeline.

## Current Important Behavior

- Scanner JSON may include UTF-8 BOM; Rust parsing must tolerate it.
- Tauri resource lookup must support bundled `_up_` resource layouts.
- Quick scan passes `-SkipCommonRoots`; Deep scan includes common-root drilldowns.
- `C:\Windows`, `C:\Recovery`, and recycle-bin/system paths should not become ordinary cleanable suggestions.
- Permission-denied paths are expected for normal users and should be surfaced as notes, not treated as scan failure.
- Reparse points are skipped to avoid double-counting or counting virtual/cloud/linked locations as local C drive usage.
- Result health checks should flag privacy/upload anomalies, unsafe system-managed classification, empty recommendations, unreadable paths, and skipped reparse points.
- `load_latest_report` loads the newest enriched local `scan-report-*.json`, registers it as a completed task, and lets report actions work after app restart.
- `copyReportPaths` has been GUI-tested and should show `报告路径已复制。`.
- Backend report generation sorts by review usefulness before truncating to 80 recommendations: low-risk cache, app-managed data, user-data, uninstall/migration, then system-managed guidance.
- Broad root summaries such as `C:\Users`, `C:\Program Files`, and `C:\Windows` should not crowd out more specific review candidates.
- Scan companion and activity feed are the intended surfaces for long-scan waiting feedback; keep future scan-waiting UX inside those surfaces unless there is a clear product reason to redesign.

## Development Commands

Install and run from source:

```powershell
npm install
npm run tauri:dev
```

Fast verification while iterating:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\Invoke-ProjectChecks.ps1 -SkipTauriBuild -SkipPackage -SkipChecksum
```

Full verification:

```powershell
npm run verify
```

Targeted checks:

```powershell
npm test
npm run build
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\Test-SafetyBoundary.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\Test-ScannerContract.ps1
```

Release executable and portable package:

```powershell
npm run tauri:build
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\New-PortablePackage.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\New-ReleaseChecksum.ps1 -ArtifactPath ".\src-tauri\target\release\windows-c-drive-cleanup-advisor.exe,.\dist\windows-c-drive-cleanup-advisor-0.1.0-windows-x64.zip"
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\Test-PortablePackage.ps1
```

Note: `npm run build` refreshes `dist`, so regenerate the portable zip and checksums after frontend builds when release artifacts matter.

## Testing Expectations

For most code changes, run the fast verification script. For frontend-only changes, at minimum run `npm test`, `npm run build`, and `git diff --check`.

For Rust/scanner/safety changes, run:

- `scripts\Test-SafetyBoundary.ps1`
- `scripts\Test-ScannerContract.ps1`
- Rust tests through `scripts\Invoke-ProjectChecks.ps1`

For release-facing changes, also run:

- `npm run tauri:build`
- `scripts\New-PortablePackage.ps1`
- `scripts\New-ReleaseChecksum.ps1`
- `scripts\Test-PortablePackage.ps1`

Manual smoke remains necessary before public release:

- Launch release exe on Windows.
- Confirm Chinese UI and safety ledger.
- Confirm Quick scan default.
- Start/cancel a scan as normal user.
- Run a completed scan.
- Open Markdown/JSON reports.
- Copy report paths.
- Restart the app and load the latest local report.
- Confirm result guide and result health panel.
- Confirm `privacy.uploaded` is `false`.
- Confirm system-managed items are blocked guidance.

## UI/UX Direction

This is an operational desktop tool, not a marketing page. Prefer dense, calm, scannable UI.

Use restrained controls, clear status messages, and practical affordances. Do not add decorative UI that hides the safety purpose.

Scan waiting issue from real testing:

- Quick scan can sit at 18%, jump to 65%, then sit at 65% for a while.
- If scan time cannot be shortened, improve waiting experience.
- First-pass mitigation is implemented: stage timers, stage reasons, next-step cues, rotating safety tips, backend heartbeat markers, activity feed, and idle-stage fallback messages.
- Future v0.2 ideas are in `docs/PRODUCT_PLAN.md`: richer scanner events, partial-summary preview, and possible interactive companion/pet.

If implementing waiting-experience work:

- Keep scan read-only.
- Keep cancel available.
- Explain long stages as active work, not a frozen app.
- Avoid slowing the scanner.
- Add tests for stalled-progress messaging or scan event interpretation.

## Release Artifact Notes

Portable package scripts currently include:

- `windows-c-drive-cleanup-advisor.exe`
- `_up_\scripts\Scan-CDriveCleanupAdvisor.ps1`
- `README.md`
- `USER-GUIDE.md`
- `AGENT.md`
- `docs\RELEASE_CHECKLIST.md`
- `docs\PRODUCT_PLAN.md`
- `docs\RELEASE_NOTES_0.1.0.md`
- `docs\SMOKE_TEST_REPORT_TEMPLATE.md`

If adding new release docs, update both:

- `scripts/New-PortablePackage.ps1`
- `scripts/Test-PortablePackage.ps1`

## Encoding Notes

Some PowerShell output may display UTF-8 Chinese as mojibake depending on console encoding. Before assuming source text is corrupted, verify with Node or another UTF-8-aware reader.

Example:

```powershell
node -e "const fs=require('fs'); const s=fs.readFileSync('src/App.tsx','utf8'); console.log(s.includes('扫描'), /\\u93b5|\\u59dd|\\u678b/.test(s));"
```

## Editing Guidance

- Keep changes scoped to the requested behavior.
- Preserve the read-only safety model.
- Prefer existing local patterns over new abstractions.
- Add focused tests for changed behavior.
- Do not rely on generated release artifacts as source of truth; source files and scripts are authoritative.
- Do not revert unrelated user changes in the working tree.

## Next Best Work

Priority order for the next thread:

1. Preserve the current `v0.1` package state: if any release docs or packaged files change, run `npm run verify` so `dist\windows-c-drive-cleanup-advisor-0.1.0-windows-x64.zip` and `dist\checksums.txt` are fresh.
2. Commit/tag the verified `v0.1` release state when the user is ready.
3. Consider result-review polish from `docs\PRODUCT_PLAN.md`: aggregate repeated cache files under the same directory, for example NVIDIA `DXCache`, so users see one folder-level candidate instead of many similar file rows.
4. Keep the scan-waiting companion/pet idea in planning for v0.2; the `v0.1` smoke/release checklist is now closed.

## Current Roadmap

- `v0.1`: Read-only GUI, risk groups, Markdown/JSON reports, latest-report loading, result health, scan waiting feedback, portable package, release notes.
- `v0.2`: Scan waiting polish, result-review aggregation, signed release planning, installer packaging.
- `v0.3`: Experimental low-risk cache cleanup allowlist using `reportId + candidateIds` and recycle-bin-first behavior.
- `v1.0`: Signed or clearly unsigned release, installer, portable zip, checksums, complete release verification guide.
