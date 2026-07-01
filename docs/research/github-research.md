# GitHub Research Notes

Research date: 2026-06-20

Scope: Windows C drive disk pressure investigation, read-only disk usage scanning, cleanup recommendation tools, and related Windows system cleanup scripts.

## Positioning

The strongest gap found is a tool that is advisory first: scan the real local C drive, skip virtual/reparse-point paths, rank likely reclaim opportunities, explain risk, and leave final cleanup decisions to the user.

Most existing projects are either:

- visual disk analyzers that show size but do not understand Windows cleanup risk;
- cleanup scripts that actually delete files or run maintenance actions;
- narrow tools for one system area, such as Windows Installer or WSL VHDX.

## Relevant Projects

### WinDirStat

Repository: https://github.com/windirstat/windirstat

Useful ideas:

- Mature Windows disk usage analyzer.
- Directory tree plus file-extension breakdown plus treemap visualization.
- Supports scanning drives, folders, and command-line targets.

How this project differs:

- WinDirStat is visual-first; our project should be advice/risk-first.
- We can borrow the idea of sortable size trees, but add Windows-specific classifications such as cache, chat data, WinSxS, pagefile, WSL, and installed applications.

### Squirreldisk

Repository: https://github.com/adileo/squirreldisk

Useful ideas:

- Rust + React + Tauri desktop app.
- Cross-platform disk scanner.
- Sunburst chart and drag/drop collection of deletion candidates.
- Auto-update via app launch.

How this project differs:

- Good reference if we build a GUI later.
- Avoid starting with delete workflows; keep the first release read-only.

### Disk Analyzer

Repository: https://github.com/jguida941/disk-analyzer

Useful ideas:

- CLI and GUI modes.
- Top folders, file types, recently modified files, and suspicious growth detection.
- `--folders`, `--types`, `--recent`, `--suspicious` style CLI flags.

Potential feature to add:

- `--suspicious` equivalent for giant logs, updater folders, cache folders, and unexpected single files.

### ComputerCleanup

Repository: https://github.com/tomskovich/ComputerCleanup

Useful ideas:

- PowerShell module organization.
- Separate commands for browser cache, SoftwareDistribution, Teams cache, Disk Cleanup Manager, system files, and user profiles.

Important caution:

- This is an action/cleanup module. Some actions stop processes or clear data. Our project should cite these categories as possible advice, not execute them by default.

### ChadSimmons Clean-SystemDiskSpace

File: https://github.com/ChadSimmons/Scripts/blob/main/Windows/Clean-SystemDiskSpace.ps1

Useful ideas:

- Risk-prioritized cleanup sequencing.
- Integrates Windows Disk Cleanup Manager categories.
- Checks free space between sections.
- Handles Windows Update files, Delivery Optimization, BranchCache, Windows Error Reporting, and system restore points.

Important caution:

- It can remove files and invoke DISM cleanup actions. Our project should keep those as recommended manual steps.
- Avoid recommending `/ResetBase` as a default.

### Invoke-WindowsDiskCleanup

File: https://github.com/adbertram/Random-PowerShell-Work/blob/master/File-Folder%20Management/Invoke-WindowsDiskCleanup.ps1

Useful ideas:

- Enumerates many classic cleanmgr volume cache categories.
- Provides a wrapper pattern around built-in Windows Disk Cleanup.

Potential feature to add:

- Report which cleanmgr categories exist and explain what they generally clean, without enabling them automatically.

### WSL Issue 4699

Issue: https://github.com/microsoft/WSL/issues/4699

Useful ideas:

- WSL2 VHDX files can stay large after deleting Linux files.
- Common manual pattern: `wsl --shutdown`, then compact the VHDX.
- Export/import can move WSL distributions to another disk.

Project rule:

- Treat `ext4.vhdx` as user-managed system data. Do not delete. Recommend compact/migrate only with explicit user consent.

### InstallerClean

Repository: https://github.com/no-faff/InstallerClean

Useful ideas:

- Narrow, focused UX for `C:\Windows\Installer`.
- Uses Windows Installer API to decide what is no longer needed.
- MIT license, CI, GitHub Release, no telemetry positioning.

Project rule:

- Keep `C:\Windows\Installer` as "do not manually delete".
- If we ever support this area, use Windows Installer APIs and a quarantine/recycle-bin workflow, not raw deletion.

### PSCacheCleaner

Repository: https://github.com/ikkxeer/PSCacheCleaner

Useful ideas:

- Windows cache cleanup categories.
- PowerShell packaging and versioned README.

Important caution:

- It automates cleanup. Our project should remain advisory by default.

## Design Takeaways

1. Keep the first release as a read-only scanner and Markdown/JSON reporter.
2. Explicitly skip reparse points to avoid double-counting or counting virtual locations.
3. Add category labels:
   - low-risk cache;
   - app-managed cleanup;
   - user data;
   - uninstall/migrate;
   - system-managed, do not manually delete.
4. Add confidence and risk fields to each recommendation.
5. Provide manual steps, not destructive commands.
6. Keep a separate `--admin-details` path for DISM and system-managed diagnostics.
7. For future GUI, consider Tauri because Squirreldisk demonstrates a working Rust + React desktop model.
8. For distribution, provide:
   - portable zip;
   - GitHub Releases;
   - signed PowerShell script if possible;
   - optional compiled GUI later.

## Avoid

- Do not use TrustedInstaller escalation tools.
- Do not take ownership of WinSxS or Windows Installer.
- Do not ship one-click cleanup as the default path.
- Do not delete chat app message folders directly.
- Do not treat pagefile, hiberfil, WSL VHDX, or Installer folder as ordinary files.
