# AGENT.md

Reusable project guidance for agents working on `windows-c-drive-cleanup-advisor`.

Last updated: 2026-09-30

## Document Authority

This file is the single product authority for data, safety, commands, and release gates; where any other document disagrees, this file wins. The related documents are fixed by role, not by duplication:

- `AGENTS.md` is a discovery entry for agents only. It carries no product rules of its own and points back here.
- `docs\release\RELEASE_CHECKLIST.md` is the living release template. Each released version also keeps a snapshot `docs\release\RELEASE_CHECKLIST_<version>.md` — `Test-PortablePackage.ps1` requires that exact filename inside the portable zip, so historical snapshots are never deleted.
- The classification lists are single-sourced in `src-tauri\src\classify.rs` (`SYSTEM_MANAGED_RULES` / `SYSTEM_MANAGED_DOC_TOKENS` plus `CATEGORY_PRIORITY`). `README.md`, `SKILL.md`, and `references\windows-cleanup-heuristics.md` are derived copies, locked by the cargo test `system_managed_list_is_documented_consistently`; edit the rules table first, then mirror the documents.
- `docs\research\2026-09-25-refactor-or-continue.md` and `docs\research\github-research.md` are dated decision/research snapshots: history for audit, not current guidance.

## Current Handoff Snapshot

`v0.2.0` is published as a public MIT-licensed, unsigned Windows x64 portable release on 2026-08-10. The main read-only scan/report workflow is implemented; v0.2 adds directory-level cache aggregation, version-contract validation, and a Windows GitHub Actions fast-verification gate. The source repository is `https://github.com/JWLuo0719/windows-c-drive-cleanup-advisor`; the release tag is `v0.2.0`.

Current work is a local `v0.3.0` release candidate on `codex/v0.3-release-prep`; it has not been published. On 2026-09-30, a clean `npm ci` followed by the 15-step `npm run verify` passed (0 vulnerabilities, 45 frontend tests, 84 Rust tests, release build/package/checksum and extracted scanner smoke). A separate opt-in recycle test moved a disposable 1 KB file into the Windows Recycle Bin, confirmed by Shell view count 0 → 1. A freshly extracted portable executable stayed running with a main window titled `Windows C 盘清理顾问` and closed normally. Full GUI interaction and cleanup through the live backend remain pending. `docs\release\RELEASE_CHECKLIST_0.3.0.md` records current evidence and open gates. The 2026-09-26 results below are historical implementation evidence.

After the checklist was updated, a second full verify attempt failed during release compilation in the seed-tree performance step because the host had about 417 MB free virtual memory (Windows os error 1455: page file too small). No code changed after the successful full run. The executable was repackaged with the final documents; checksums were regenerated and `Test-PortablePackage.ps1` passed. Re-run the full clean-install gate with sufficient free virtual memory before publication. Do not change the user's page-file settings as part of verification.

Phase 0 correctness hardening landed 2026-09-25/26 as the first step of the hybrid restart plan (`docs\research\2026-09-25-refactor-or-continue.md`). It fixes task-state races (terminal-state freeze, queued-window cancel, PID registration timing), adds a stdout watchdog with stall detection (`taskkill /T /F`), and adds ring pruning plus a `latest-report.txt` index. An adversarial review pass then confirmed three defects in that diff and they are fixed: the stdout reader now reads raw lines and decodes UTF-8 with GBK fallback (`encoding_rs`) so PowerShell 5.1 OEM/GBK progress lines cannot freeze progress or the watchdog activity stamp; report pruning counts only directories containing `scan-report-*.json` so leftover garbage directories cannot evict real reports; watchdog tests use `checked_sub` so they cannot panic on machines with under one hour of uptime. IPC contract (6 commands, `scan-progress`, camelCase, schemaVersion 0.1.0), safety-boundary source strings, and the read-only product boundary are unchanged.

Phase 1 structural refactor landed 2026-09-26 as the second hybrid-plan step (`docs\research\2026-09-25-refactor-or-continue.md` Phase 1). `src-tauri\src\lib.rs` is split into `commands`/`tasks`/`scanner`/`classify`/`report`/`errors` modules with the 6-command IPC surface and serialization fields unchanged. Classification rules are declarative `PathRule` tables in `classify.rs` with an explicit `CATEGORY_PRIORITY` short-circuit order (system-managed → low-risk-cache → app-managed → uninstall-or-migrate → user-data) and drive-letter parameterization (`strip_drive_root`); the `update`/`qq` markers are tightened from v0.2 substring matching to complete directory-segment matches (`SegmentSeq`) with negative-case unit tests so similar names (`update-notes`, `qq-backup`) cannot be mislabeled — mislabeling as low-risk-cache is the most dangerous direction. The `[WCDCA_PROGRESS] {percent}|{code}` protocol is now a three-way aligned constant table (PS emitter in `Scan-CDriveCleanupAdvisor.ps1`, Rust parser in `scanner.rs`, TS consumer `PROGRESS_PCT` in `scanStages.ts`) and `Test-ScannerContract.ps1` statically cross-checks all three maps; the frontend consumes only `percent` and never parses progress message text (`message.includes` is lint-banned). `src\App.tsx` is split into a composition shell plus `useScanSession` (sole owner of the scan-session state machine, with a `listen()` cleanup race fix and `resetScanTracking()` replacing the v0.2 repeated quadruple resets) and `ScanCompanion`/`SummaryPane`/`RecommendationList` presentational components — `App.test.tsx` passed unchanged through the split. ESLint 10 (flat config), Prettier 3, and vitest v8 coverage thresholds (80/75/85/80) are wired into `Invoke-ProjectChecks.ps1`. Classification marker lists are unit-tested for consistency across `classify.rs`, `README.md`, `SKILL.md`, and `references\windows-cleanup-heuristics.md`.

Phase 2 native scan kernel landed 2026-09-26 as the third hybrid-plan step (`docs\research\2026-09-25-refactor-or-continue.md` Phase 2). The default scan path is now an in-process Rust kernel (`src-tauri\src\kernel.rs`) that replaces the PowerShell double-walk with a single drive walk (top rows and large-file candidates collected in one traversal; deep drilldowns remain per-anchor, matching PS); `WCDCA_SCAN_KERNEL=powershell` keeps the v0.2 PS scanner as an explicit fallback. The progress protocol shape is fully preserved (frozen stage codes, heartbeat codes with path suffixes, `[WCDCA_PROGRESS] {percent}|{code}` markers, cancellation cooperative via the progress-sink return value), and the raw JSON/Markdown reports keep the PS property names and frozen safety statements ("Read-only scan: yes" / "This tool did not delete, move, or modify files."). The test-only `wcdca-scan` binary (declared in `Cargo.toml`, not bundled) drives dual-kernel A/B equivalence assertions in `Test-ScannerContract.ps1` (top-row field equality, large-file rows, hardlink per-link accounting `0.06|2|1|0`, junction skip count, scanError 2:1 dual-walk ratio with ACL-denied ListDirectory injection, native `scanErrorTotal` truncation semantics) and the performance gate in `Measure-ScanPerformance.ps1`. Kernel differences are documented in `docs\release\KERNEL_MIGRATION.md`: scanErrors once per path (native) vs twice (PS dual walk, LargeFiles* stages only exist on the PS side), `scanErrorTotal` present only in the native JSON, half-away-from-zero vs banker's SizeGB rounding, deterministic exact-byte descending sort vs PS SizeGB ties, LastWriteTime unix-ms vs .NET DateTime, pagefile peak and AutomaticManagedPagefile not read via WMI natively. Hardlink dedup by file id was dropped (std `MetadataExt::file_index` is unstable) — both kernels count per link. Performance gates passed on the **release** build: seed tree 144.7x over PS (4806 files) and real `C:\` Quick 35.7s (< 60s acceptance budget; the debug build measures 110s — benchmarks must build `--release`). `Test-SafetyBoundary.ps1` gained dual-kernel assertions: kernel process-launch whitelist (Dism.exe only), no PS launch surface in `kernel.rs`, no delete/rename APIs outside `#[cfg(test)]`, native-by-default selector with opt-in PS fallback, `wcdca-scan` declared but never bundled.

Phase 3 report discovery layer landed 2026-09-26 as the fourth hybrid-plan step (`docs\research\2026-09-25-refactor-or-continue.md` Phase 3). The enriched report carries three additive fields (`topRows`/`drilldowns`/`largeFiles`, `#[serde(default)]` so pre-Phase-3 reports still load) that pass the raw kernel rows through to the UI. The frontend gains a read-only treemap (`TreemapView.tsx`: d3-hierarchy squarify + canvas, aggregate-to-drilldown navigation bounded by report data depth, residual buckets for unlisted size, card list retained as the full detail view), recommendation search/path filtering with size/risk/confidence sorting (`filterAndSortRecommendations`), and `HistoryPanel.tsx` (local report list, load-by-id, and two-report delta comparison via pure-frontend `compareReports`). `styles.css` is tokenized: 133 hex literals replaced by 60 semantic `--color-*` tokens plus spacing/radius/shadow tokens, with identical values (zero visual change). The IPC surface grows additively 6 → 8 commands: `list_reports` (summaries sorted by createdAt desc, latest flagged, broken entries skipped) and `load_report_by_id` (scan id must parse as a UUID — path-traversal inputs are rejected). Two real defects were caught by the new tests and fixed: the treemap layout took `leaves().filter(depth===1)`, which drops directories that have children (nearly every real row — the map rendered empty rectangles), now fixed to the focus node's direct children; `formatDuration` now clamps NaN/Infinity to 0 instead of rendering "NaN 秒". Scope guard unchanged: no drag-drop, no right-click delete, no live filesystem access, no ADS, no binary snapshots — the treemap is aggregate display only.

Phase 4 engineering guarantees landed 2026-09-26 as the fifth hybrid-plan step (`docs\research\2026-09-25-refactor-or-continue.md` Phase 4). CI is a single workflow (`.github\workflows\ci.yml`, `verify.yml` deleted): a fast job on every trigger runs `Invoke-ProjectChecks.ps1 -SkipTauriBuild -SkipPackage -SkipChecksum`, a full job (push to `main` only) runs the `npm run verify` gate, checks `dist\checksums.txt` with `sha256sum -c` under bash, and uploads artifacts; `permissions` is pinned to `contents: read` and `Swatinem/rust-cache@v2` caches cargo (the workflow edit is untested until the next push — GitHub Actions does not run locally). `New-ReleaseChecksum.ps1` now emits GNU sha256sum text format (`hash` + two spaces + repo-root-relative forward-slash path, UTF-8 without BOM, LF endings) so `sha256sum -c dist/checksums.txt` passes directly, and `Test-PortablePackage.ps1` parses the two-token format with a leaf-name fallback for legacy lines. Shared PowerShell helpers (`Assert-True`, `Get-FreeDriveLetter`, `Get-Sha256Hex`) are single-sourced in `scripts\Common.ps1` and dot-sourced by five scripts. `Scan-CDriveCleanupAdvisor.ps1` gains `scanErrorTotal` (pre-truncation error total, aligned with the native kernel; asserted in `Test-ScannerContract.ps1`) and replaces the blanket `$ErrorActionPreference = "SilentlyContinue"` with `Stop`: every expected-to-fail read carries an explicit `-ErrorAction SilentlyContinue` (full-IO audit — enumeration points, CIM queries, plus the `hiberfil.sys` probe added in this pass), so unexpected errors surface instead of being swallowed, while report-write failures abort with `exit 1`. `github-research.md` gains a comparison matrix (SpaceSniffer, WinDirStat, WizTree, TreeSize Free, BleachBit, Czkawka vs this project) that states the advice-plus-risk column as the product gap. Document authority is declared once in `AGENT.md` §Document Authority: `AGENTS.md` is a discovery entry, `RELEASE_CHECKLIST_<version>.md` snapshots are required portable-zip entries (never deleted) with `RELEASE_CHECKLIST.md` as the living template, classification lists are single-sourced in `classify.rs` and locked by the `system_managed_list_is_documented_consistently` cargo test, and research docs are dated snapshots.

Phase 5 experimental recycle-bin cleanup landed 2026-09-26 as the sixth hybrid-plan step (`docs\research\2026-09-25-refactor-or-continue.md` Phase 5, the final phase). The write surface is `src-tauri\src\cleanup.rs` — the project's only deletion executor. Plan/execute inputs are `reportId + candidateIds` only: paths are re-resolved server-side from the stored enriched report (the UI never passes paths), and the v0.3 allowlist accepts only `category == "low-risk-cache" && cleanable`. Plan-first: `plan_cleanup` and `execute_cleanup` share `build_plan_with`, which applies the allowlist, rejects system-managed/blocked entries via the Phase 1 data-driven `reclassify_gate` (also re-run at deletion time), enforces the conservative recycle budget (disk free / 10), and runs triple path validation (symlink + parent-chain reparse rejection, original-vs-canonical file-id identity recheck via `GetFileInformationByHandle`, canonical reparse recheck with `FILE_FLAG_BACKUP_SEMANTICS` directory handles). Execution goes through an injected `Recycler` trait: the production `ShellRecycler` uses `SHFileOperationW FO_DELETE | FOF_ALLOWUNDO` on a trailing-backslash-stripped, double-NUL-terminated `pFrom` (a trailing backslash would bypass the recycle bin and delete permanently), checks `fAnyOperationsAborted`, and confirms the original path is gone; `SHQueryRecycleBinW` probes first and a disabled recycle bin rejects the whole request. Every item outcome is appended to a JSONL action log read back by `get_action_log`. The frontend renders candidate checkboxes only on cleanable low-risk-cache cards, a plan panel (items, rejected reasons, budget, confirm), an outcome panel (recycled/failed/rejected counts, stale-report notice), and an audit-log section in `HistoryPanel`; cleanup state resets whenever the report changes or a checkbox toggles. IPC grows 8 → 11 commands (`plan_cleanup`, `execute_cleanup`, `get_action_log`). Rust tests grow 70 → 84 (14 cleanup tests: allowlist / system-managed / junction / missing-path / over-budget rejections, FakeRecycler outcomes + JSONL entries, delete-time recheck, log ordering + corrupt-line skip, and a real recycle-bin smoke gated by `WCDCA_RECYCLE_SMOKE=1` so default and CI runs stay hermetic). Frontend tests grow 42 → 45 (checkbox gating on low-risk-cache+cleanable, plan-first flow asserting `planCleanup("scan-123", ["npm-cache"])` / `executeCleanup("scan-123", ["npm-cache"])` with rejection rendering, audit-log expansion). `Test-SafetyBoundary.ps1` gains section 5: the delete executor (`SHFileOperationW`/`FO_DELETE`) is confined to `cleanup.rs`; `remove_file`/`remove_dir`/`fs::rename` are absent from every other module's product code (`report.rs` exempt — it rotates only its own report directories; the kernel 3.3 assertions are untouched); `cleanup.rs` must keep its safety markers (`FOF_ALLOWUNDO`, `reclassify_gate`, `GetFileInformationByHandle`, `from.push(0)` double-NUL, trailing-backslash strip, execute-via-`build_plan_with`); no elevation (`runas`/`ShellExecute` in Rust, `RunAs` in ps1 — the scanner script is included and the checker skips itself) and no network APIs (Rust product code and ps1 scripts). Probe-verified: a temporary module containing `fs::remove_file` was rejected by the new assertion and removed afterwards. Scope guard unchanged: default user privileges, no elevation helper, no non-allowlist categories, no permanent-delete fallback — blocked candidates surface reasons instead of being overridden, and scan/report paths stay read-only.

Historical verification of the Phase 5 working tree (2026-09-26):

- `powershell -File scripts/Invoke-ProjectChecks.ps1` → exit code 0 (full gate: release version contract `0.2.0`, safety boundary incl. Phase 5 deletion-surface assertions, scanner contract incl. A/B scan-error ratio, seed-tree performance gate, frontend lint, frontend formatting, frontend unit tests with coverage, frontend build, npm audit, `cargo test`, `cargo check`, Tauri release build, portable packaging, checksum, portable package validation incl. runtime smoke of the rebuilt package with the `EAP=Stop` scanner — all `[OK]`; this is still NOT the clean-install `npm run verify` gate)
- `cargo test` → `test result: ok. 84 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out` (70 prior tests plus 14 cleanup tests covering the plan/execute/action-log surface)
- Real recycle-bin smoke: `WCDCA_RECYCLE_SMOKE=1 cargo test recycle_smoke` → ok, with the recycled file confirmed present as `C:\$Recycle.Bin\S-1-5-21-*\$R*.bin` via Shell COM query on 2026-09-26 (proves SHFileOperationW lands items in the recycle bin rather than deleting permanently; the test self-skips without the env var)
- `npx vitest run` → `Test Files 4 passed (4)`, `Tests 45 passed (45)` (42 prior tests plus 3 Phase 5 tests: cleanup checkbox gating, plan-first preview + execute flow with IPC argument assertions, audit-log expansion)
- `npm run build` (`tsc && vite build`) → 0 type errors
- `npm run test:coverage` → statements 85.00 / branches 79.18 / functions 89.37 / lines 85.43 against thresholds 80/75/85/80 (`src/tauri.ts` IPC shim excluded — its call surface is exercised through the `./tauri` mock in `App.test.tsx`)
- `npm run lint` → 0 problems (includes the ban on `message.includes` progress-text parsing and `set-state-in-effect`)
- `npm run format:check` → all checked files formatted
- `powershell -File scripts/Test-SafetyBoundary.ps1` → `[OK] Safety boundary checks passed` (dual-kernel assertions plus the Phase 5 section 5: delete-executor confinement, read-only product code outside `cleanup.rs`/`report.rs`, cleanup safety markers, no elevation, no network; the destructive-verb blacklist covers all ps1 scripts including `Measure-ScanPerformance.ps1`)
- `powershell -File scripts/Test-ScannerContract.ps1` → `[OK] Scanner contract checks passed` including `[OK] A/B scan-error ratio verified (PS 2 : native 1 per path)` (three-way protocol-map equality, marker-format alignment, frontend text-coupling ban, subst-drive E2E, dual-kernel A/B equivalence)
- `powershell -File scripts/Measure-ScanPerformance.ps1` → (Phase 5 runs inside Invoke-ProjectChecks, twice that day) `[PERF] seed-tree dirs=300 files=4806 native=0.06s ps=2.78s speedup=45.7x gates=10x,30s` and a final wrap-up run `[PERF] ... native=0.06s ps=2.83s speedup=44.9x`, both followed by `[OK] Seed-tree performance gate passed` (Phase 4 run measured 28.1x, Phase 3 46.4x, Phase 2 144.7x on an idle machine; every run clears the 10x gate — speedup varies with disk cache/load); `-RealDrive -Drive C` was not re-run in the Phase 5 pass — the latest measured value remains Phase 2/3's `[PERF] real-drive quick drive=C:\ native=35.7s budget=60s` `[OK]` (release build; outputs preserved in `docs\release\KERNEL_MIGRATION.md`)

The earlier full `npm run verify` passed on 2026-09-25 at the Phase 0 mid-point. The 2026-09-30 clean-install run now supersedes that release-gate gap. `npm audit fix` advanced transitive `undici` from 7.29.0 to 7.30.0 after a new moderate advisory, and the final audit reported 0 vulnerabilities. `Test-ScannerContract.ps1` now uses .NET directory ACL APIs for its temporary error injection because Windows PowerShell 5.1 can fail to autoload `Microsoft.PowerShell.Security` when the host also exposes PowerShell 7 module paths. An npm audit TLS disconnect occurred on one attempt; the successful final run completed the audit. Source-level checks and startup evidence still do not replace manual GUI workflow verification.

Known limitations accepted from the adversarial review (both split 1-1, not confirmed):

- The DISM component-store analysis phase can be structurally silent; if it exceeds the 30-minute stall timeout the watchdog would kill a healthy scan. Not observed in practice; planned mitigation is a heartbeat in the scanner's DISM section, not a longer timeout.
- If writing `latest-report.txt` fails and cleanup of the stale index also fails, loading the latest report may return the previous report until the next successful index write. Stale but valid; self-heals.

Kernel-level known differences (Phase 2, documented in `docs\release\KERNEL_MIGRATION.md`): scan-error counting (1x native vs 2x PS per failing path), SizeGB rounding ties, large-file LastWriteTime representation, and WMI-only fields (pagefile peak usage, AutomaticManagedPagefile) being null in the native kernel. All are read-report-only divergences; none change the safety model.

Untested environments: all evidence above is from Windows 11 Home China 10.0.26200 on one machine. The merged CI workflow (fast job on PR, full job with `sha256sum -c` on push to `main`) is written but has not yet run on GitHub Actions. Windows 10, non-Chinese locales (OEM code pages other than GBK), machines without adjacent `_up_` scanner resources, ReFS/non-NTFS volumes, and network drives under the native parallel walker are untested. The real recycle-bin smoke has run on this machine with one small disposable file at a time; bulk/large-item recycle behavior, a full or disabled Recycle Bin, and read-only-permission failures are not exercised against real Shell state. The cleanup UI has only been exercised through mocked IPC in jsdom, never through a live backend session.

Latest full verification command for releases (clean install required):

```powershell
npm run verify
```

The release gate must run from a clean `npm ci` install. It checks version agreement, safety boundary, scanner contract, frontend lint/formatting/coverage tests/build, dependency audit, Rust tests/check, Tauri release build, portable package creation, checksum generation, and portable runtime validation. `Invoke-ProjectChecks.ps1` must fail on every non-zero child process exit code.

Latest release checksums are in `dist\checksums.txt`. Do not hard-code them in this file because `AGENT.md` is included in the portable zip; embedding the zip hash here would make the package self-referential and stale after every repack.

Current v0.2 GUI evidence is recorded in `docs\release\SMOKE_TEST_REPORT_2026-08-12.md`; historical v0.1 evidence remains in `docs\release\SMOKE_TEST_REPORT_2026-06-30.md`.

## Project Purpose

This Windows C drive advisor scans and reports read-only, diagnoses disk pressure, and ranks local space candidates. The v0.3 source also has a separate, opt-in, plan-first Recycle Bin action for allowlisted low-risk caches.

Current product shape:

- Tauri v2 desktop shell.
- React/Vite/TypeScript frontend.
- Rust owns the narrow IPC boundary.
- The native Rust kernel is the default scanner; bundled PowerShell remains an explicit fallback. Both write local Markdown and enriched JSON reports.
- Quick scan is the default; Deep scan adds common-root drilldowns.
- Result guide and result health panel help users judge scan output.
- Scan companion and activity feed keep long scans visibly active.
- Latest local report can be loaded after app restart.
- Enhanced report generation prioritizes specific review candidates before broad root-folder summaries.
- Experimental cleanup is isolated in `src-tauri/src/cleanup.rs`, accepts stored report and candidate IDs only, and records outcomes locally.
- Portable release zip includes executable, bundled scanner resource, README, user guide, `AGENT.md`, MIT license, versioned release checklist, product plan, release notes, and smoke-test template.

## Non-Negotiable Safety Boundary

Scanning, reporting, and report browsing must stay read-only. Do not add any other code that deletes, moves, truncates, uninstalls, uploads, changes settings, disables services, modifies registry state, or silently runs cleanup commands. The only approved write capability outside local reports is the explicit, plan-first Recycle Bin action in `cleanup.rs` for allowlisted low-risk caches.

Keep these constraints unless the user explicitly requests a scoped design change and the safety model is updated first:

- No generic Tauri shell permission for the frontend.
- No arbitrary command execution from UI input.
- Native Rust scanning is the default; the PowerShell fallback launches only the bundled scanner with fixed arguments.
- Reports stay local; no telemetry or upload.
- System-managed items remain blocked guidance, not cleanup tasks.
- Cleanup inputs are `reportId + candidateIds` only; paths are resolved from the stored report, reclassified, and revalidated at execution time.
- Only `low-risk-cache && cleanable` may enter a cleanup plan. Use the Windows Recycle Bin only, with no permanent-delete fallback or elevation helper; log every outcome.

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
- `docs/release/RELEASE_CHECKLIST_0.3.0.md`: current release-candidate gates and manual smoke tests; the 0.2.0 snapshot remains historical.
- `docs/product/PRODUCT_PLAN.md`: scan waiting experience and result-review planning.
- `docs/release/RELEASE_NOTES_0.3.0.md`: current candidate feature list and limitations.
- `docs/release/SMOKE_TEST_REPORT_2026-08-12.md`: latest v0.2 GUI smoke-test evidence.
- `SKILL.md`: cleanup-advisor domain heuristics and reporting style.

For code changes, inspect the relevant files:

- `src/App.tsx`: UI composition shell.
- `src/useScanSession.ts`: scan-session state machine (progress, activity feed, report loading/actions).
- `src/scanStages.ts`: progress-percent contract (`PROGRESS_PCT`), scan defaults, stage copy.
- `src/reportUtils.ts`: frontend result grouping, summaries, and health checks.
- `src/tauri.ts`: frontend IPC wrappers.
- `src-tauri/src/lib.rs`: module wiring only (`commands`/`tasks`/`scanner`/`classify`/`report`/`errors`).
- `src-tauri/src/classify.rs`: declarative classification rules and category priority.
- `src-tauri/src/scanner.rs`: scanner launch, progress-marker parsing, stdout decoding.
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
- Classification is rule-table driven (`src-tauri\src\classify.rs`): explicit `CATEGORY_PRIORITY` short-circuit order, `MatchKind` marker kinds, and drive-letter-agnostic matching. `update`/`qq` match complete directory segments only; keep the negative-case tests when touching rules.
- The progress protocol `[WCDCA_PROGRESS] {percent}|{code}` is a frozen three-way contract (PS emitter constants / Rust `scanner.rs` constants / TS `PROGRESS_PCT`); the UI must consume `percent` only and never parse message text.
- `useScanSession` owns all scan-session state; keep `resetScanTracking()` as the single teardown entry and preserve the `listen()` disposed-flag cleanup fix when touching subscription logic.

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
npm run test:coverage
npm run lint
npm run format:check
npm run build
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\Test-SafetyBoundary.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\Test-ScannerContract.ps1
```

Release executable and portable package:

```powershell
npm run tauri:build
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\New-PortablePackage.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\New-ReleaseChecksum.ps1 -ArtifactPath ".\src-tauri\target\release\windows-c-drive-cleanup-advisor.exe,.\dist\windows-c-drive-cleanup-advisor-0.3.0-windows-x64.zip"
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
- For v0.3, check the cleanup plan and audit log against the real backend using disposable test data, and confirm non-allowlisted items cannot execute.

## UI/UX Direction

This is an operational desktop tool, not a marketing page. Prefer dense, calm, scannable UI.

Use restrained controls, clear status messages, and practical affordances. Do not add decorative UI that hides the safety purpose.

Scan waiting issue from real testing:

- Quick scan can sit at 18%, jump to 65%, then sit at 65% for a while.
- If scan time cannot be shortened, improve waiting experience.
- First-pass mitigation is implemented: stage timers, stage reasons, next-step cues, rotating safety tips, backend heartbeat markers, activity feed, and idle-stage fallback messages.
- Future v0.2 ideas are in `docs/product/PRODUCT_PLAN.md`: richer scanner events, partial-summary preview, and possible interactive companion/pet.

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
- `docs\README.md`
- `docs\release\RELEASE_CHECKLIST_0.3.0.md`
- `docs\product\PRODUCT_PLAN.md`
- `docs\release\RELEASE_NOTES_0.3.0.md`
- `docs\release\SMOKE_TEST_REPORT_TEMPLATE.md`

If adding new release docs, update both:

- `scripts/New-PortablePackage.ps1`
- `scripts/Test-PortablePackage.ps1`

## Encoding Notes

Some PowerShell output may display UTF-8 Chinese as mojibake depending on console encoding. Before assuming source text is corrupted, verify with Node or another UTF-8-aware reader. The scanner stdout reader inside the app decodes raw lines with UTF-8 first and GBK fallback (`decode_scanner_line` in `src-tauri/src/scanner.rs`) because PowerShell 5.1 pipes use the OEM code page; keep that path byte-tolerant so progress and the watchdog activity stamp never stop updating. PowerShell scripts with Chinese comments must keep their UTF-8 BOM, or Windows PowerShell 5.1 parses them as GBK and reports ParserError.

Example:

```powershell
node -e "const fs=require('fs'); const s=fs.readFileSync('src/App.tsx','utf8'); console.log(s.includes('扫描'), /\\u93b5|\\u59dd|\\u678b/.test(s));"
```

## Editing Guidance

- Keep changes scoped to the requested behavior.
- Preserve read-only scanning/reporting and the narrow cleanup allowlist, Recycle Bin, and audit boundary.
- Prefer existing local patterns over new abstractions.
- Add focused tests for changed behavior.
- Do not rely on generated release artifacts as source of truth; source files and scripts are authoritative.
- Do not revert unrelated user changes in the working tree.

## Next Best Work

Priority order for the next thread:

1. Preserve the published `v0.2.0` release: if release docs or packaged files change, create a new verified release artifact rather than silently replacing the published asset.
2. Publish only portable zip and checksum assets; the standalone executable needs its adjacent `_up_` scanner resources.
3. Finish the open manual gates in `docs\release\RELEASE_CHECKLIST_0.3.0.md`, especially the real-backend cleanup UI flow with controlled disposable data, before publishing v0.3.0. Keep the default-privilege allowlist, preview, explicit confirmation, Recycle Bin-only execution, and audit boundary.
4. Run the updated GitHub Actions workflow on the release branch/main and review its results. The local clean-install gate and independent portable startup passed on 2026-09-30, but CI and broader Windows environments remain unverified. The elevation helper for system categories was deliberately not built.

## Current Roadmap

- `v0.1`: Historical read-only GUI, reports, latest-report loading, result health, and scan waiting feedback.
- `v0.2`: Public MIT-licensed, unsigned Windows x64 portable release with cache-directory aggregation and verified release gates.
- `v0.3`: Experimental low-risk cache cleanup allowlist using `reportId + candidateIds` and recycle-bin-first behavior.
- `v1.0`: Signed or clearly unsigned release, installer, portable zip, checksums, complete release verification guide.
