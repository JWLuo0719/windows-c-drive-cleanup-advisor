---
name: windows-c-drive-cleanup-advisor
description: Read-only Windows C drive disk pressure investigation and cleanup recommendation workflow. Use when a user asks to diagnose low C drive space, identify large local folders, distinguish safe caches from user data and system components, handle WinSxS/pagefile/WSL/AppData questions, or produce a cleanup plan without deleting files.
---

# Windows C Drive Cleanup Advisor

## Core Rule

Never delete, move, truncate, uninstall, or change settings unless the user explicitly approves a specific action. Default to read-only scanning and ranked recommendations.

## Workflow

1. Confirm C drive free space with `[System.IO.DriveInfo]::GetDrives()`.
2. Scan top-level C drive usage while skipping `ReparsePoint` entries. This avoids counting virtual folders such as phone mirrors, OneDrive placeholders, junctions, and compatibility links as real local storage.
3. Drill into the largest real roots, usually `C:\Users\<user>`, `C:\Windows`, `C:\ProgramData`, `C:\Program Files`, and `C:\Program Files (x86)`.
4. Categorize findings:
   - Safe/low-risk cache: browser cache, GPU shader cache, package-manager caches, editor extension installers, app update installers.
   - User decision required: WeChat/QQ/Enterprise WeChat files, Downloads, Desktop, game saves, WSL distributions, cloud drive local data.
   - Uninstall/migrate only: installed apps, SDKs, CUDA, Visual Studio components, games.
   - System-managed: `WinSxS`, `Windows\Installer`, `System32`, `Windows\servicing`, `System Volume Information`, `Recovery`, `$Recycle.Bin`, `pagefile.sys`, `swapfile.sys`, `hiberfil.sys`, restore points.
5. Explain why each item is or is not safe. Provide expected reclaim ranges, not guarantees.
6. Give steps for the user to perform manually or with built-in tools. Do not present destructive commands as something to run automatically.

## Scanner

Use `scripts/Scan-CDriveCleanupAdvisor.ps1` when a deterministic read-only report is useful. It writes a Markdown report and optional JSON data.

Example:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\Scan-CDriveCleanupAdvisor.ps1 -Drive C -OutputDir .\report
```

The scanner:

- skips reparse points by default;
- reports top folders and large files;
- inspects common cache locations;
- checks pagefile and hibernation state;
- optionally runs DISM AnalyzeComponentStore when elevated;
- never deletes files.

## Key Judgments

Treat these as strong defaults:

- `C:\Windows\WinSxS`: never manually delete. Analyze with `Dism.exe /Online /Cleanup-Image /AnalyzeComponentStore`; clean only with `StartComponentCleanup`. If it fails with `0x80070005` and `STATUS_CANNOT_DELETE`, try scheduled task, reboot, `RestoreHealth`, and `sfc /scannow`.
- `C:\Windows\Installer`: never manually delete. It is needed for repair/uninstall.
- `C:\pagefile.sys`: do not delete. Recommend tuning via Windows virtual memory settings. If RAM is about 16 GB and peak pagefile use is modest, a fixed 8192 MB pagefile on C is a conservative space-saving setting.
- `C:\hiberfil.sys`: if present and the user does not need hibernation/Fast Startup, `powercfg /h off` can reclaim space, but this is a system setting change.
- NVIDIA `DXCache`, browser caches, npm/pip/Gradle/Playwright caches, and app updater folders are usually safe cache candidates.
- WeChat/QQ/WXWork files should usually be cleaned from inside the app, not by deleting message folders directly.
- WSL `ext4.vhdx` should be migrated/exported or compacted with WSL stopped; do not delete the VHDX unless the user wants to remove that distro.

## Reporting Style

Lead with the current disk pressure, then list the largest reclaim opportunities.

Use sections:

- Immediate low-risk cache candidates
- App-managed data requiring user decision
- Uninstall or migrate candidates
- System-managed items
- Suggested cleanup order

Always say explicitly that no cleanup has been performed.

## References

Read `references/windows-cleanup-heuristics.md` for detailed category rules, common paths, and failure handling.
