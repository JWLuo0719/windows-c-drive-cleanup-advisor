# Windows C Drive Cleanup Advisor

The C drive scan and report are read-only. The `v0.3.0` source also offers an optional experimental action that moves eligible low-risk cache items to the Windows Recycle Bin after a separate plan and confirmation. It does not uninstall software, upload data, or change Windows settings. The published `v0.2.0` package has no cleanup action.

## Desktop App

From source:

```powershell
npm install
npm run tauri:dev
```

The desktop app uses a Rust IPC boundary. The frontend does not receive generic shell permissions. An in-process Rust kernel scans by default; the bundled PowerShell scanner is available as an explicit fallback with fixed arguments.

## Release Checksum

After building the desktop app, create a portable zip:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\New-PortablePackage.ps1
```

Then generate SHA-256 checksums:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\New-ReleaseChecksum.ps1
```

Validate that the portable zip contains the executable, scanner resource, and docs:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\Test-PortablePackage.ps1
```

Run the safety boundary check:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\Test-SafetyBoundary.ps1
```

For release preparation, also review:

```text
docs\release\RELEASE_CHECKLIST_0.3.0.md
```

## Verify The Project

Run the full local verification pipeline:

```powershell
npm run verify
```

This includes the scan and cleanup safety boundary check, frontend tests, Rust tests, release build, portable package validation, and checksum generation.

For a quicker check while developing:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\Invoke-ProjectChecks.ps1 -SkipTauriBuild
```

## Run

Double-click:

```text
Run-CDriveCleanupAdvisor.cmd
```

Or run in PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\Scan-CDriveCleanupAdvisor.ps1 -Drive C -OutputDir .\report -IncludeJson
```

Run as administrator if you want WinSxS analysis from DISM. The scanner still works without administrator rights, but some system details may be unavailable.


## Scan Modes

Use **Quick scan** for normal checks. It skips repeated common-folder drilldowns and is the best default for a first pass.

Use **Deep scan** when you want extra detail under common user, AppData, cache, and app-managed locations. It can take longer because it performs additional read-only enumeration.

Both modes are advisory only and keep reports on this computer.

While a scan is running, the desktop app shows a **扫描陪伴** panel under the progress bar. If the percentage stays still for a while, this panel continues updating elapsed time, current-stage time, stage explanations, safety tips, and recent scan activity so the app does not feel frozen. It also shows the current focus, why that stage can be slow, and the likely next step. During long top-root sizing and large-file enumeration stages, the scanner sends low-frequency heartbeat updates such as the directory currently being counted.

## Output

Reports are written to:

```text
report\
```

In the desktop app, use **载入最近报告** to reopen the latest local report after restarting the app. After a scan completes or a recent report is loaded, use **显示 Markdown**, **显示 JSON**, **打开报告目录**, or **复制报告路径**.

The report history panel lists local reports. You can reopen one by its scan ID or compare two reports. The treemap shows the size breakdown recorded in a report; it does not browse the live filesystem.

The Markdown report lists:

- current C drive pressure;
- largest real local folders, excluding reparse points;
- common cache and app-data drilldowns when Deep scan is selected;
- large files;
- pagefile and hibernation state;
- WinSxS analysis when elevated;
- cleanup advice for the user to decide on.


## How To Read Results

First check **结果自检** in the desktop app. It confirms whether the report stayed local, whether recommendations were generated, whether system-managed items stayed blocked, and whether unreadable or linked paths need attention.

Use **优先复核体量** as an estimate of specific items worth reviewing first. It avoids adding broad roots such as `C:\Users` or blocked system-managed areas into the headline number.

Start with the **low-risk cache** group. Close the related app first and review the path and contents. Some caches can contain useful state. Only candidates explicitly marked as cleanable have the experimental Recycle Bin option.

## Experimental Recycle Bin Cleanup

This section applies to the current `v0.3.0` source and any package built from it. The published `v0.2.0` package remains advisory only.

1. Review a completed or reopened report and select only the low-risk cache candidates marked **移入回收站（实验）**. The UI cannot select system-managed items, user data, application stores, or other non-allowlisted categories.
2. Choose **计划清理**. Check the proposed items, rejection reasons, and Recycle Bin budget. Planning does not move files.
3. Choose **确认移入回收站** only after reviewing the plan. The backend checks the stored report, category, path identity, and reparse points again. Each accepted item is sent to the Windows Recycle Bin; if the Recycle Bin is unavailable, execution is rejected rather than permanently deleting the item.
4. Review each outcome and the **清理审计日志** in report history. After an attempt, the old report is stale; run a new scan before relying on its sizes or candidates.

The action uses normal user privileges and only paths accessible to that user. A failed or rejected item stays visible with a reason. The app does not offer a permanent-delete fallback or a way to override blocked categories.

Treat **system-managed** items as blocked. Do not manually delete WinSxS, Windows Installer, System32, Recovery, pagefile, swapfile, hibernation files, or protected Windows folders. Use Windows Settings, Disk Cleanup, DISM analysis, or vendor documentation instead.

Permission errors are not scan failures. Normal users commonly cannot read WindowsApps, Defender, Recycle Bin identities, and some ProgramData folders; the scanner groups those protected paths in the UI and continues.

Skipped reparse points are a safety signal. The scanner avoids counting linked, virtual, or placeholder locations as ordinary local C drive usage.

## Safety

Scanning and reporting are advisory and read-only. The separate experimental cleanup action requires a plan and explicit confirmation and is confined to eligible low-risk caches. Chat app data, system components, WSL distributions, and installed programs remain outside that action.
