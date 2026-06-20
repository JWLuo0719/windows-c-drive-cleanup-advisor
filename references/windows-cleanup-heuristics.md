# Windows Cleanup Heuristics

## Real Size vs Virtual Size

Skip directories with the `ReparsePoint` attribute during size scans. Reparse points can represent junctions, symlinks, phone mirrors, cloud placeholders, or Windows compatibility links. Counting them can double-count data or count remote/virtual content as C drive usage.

Common examples:

- `C:\Users\<user>\CrossDevice\<phone>` phone mirror entries.
- `C:\Documents and Settings` compatibility junction.
- OneDrive placeholders and cloud-backed folders.
- App junctions inside package directories.

## High-Value Cache Candidates

Usually safe to recommend after closing related apps:

- `%LOCALAPPDATA%\NVIDIA\DXCache`
- `%LOCALAPPDATA%\NVIDIA\GLCache`
- `%LOCALAPPDATA%\npm-cache`
- `%LOCALAPPDATA%\pip\cache`
- `%USERPROFILE%\.gradle\caches`
- `%LOCALAPPDATA%\ms-playwright`
- `%APPDATA%\Code\CachedExtensionVSIXs`
- Browser `Cache`, `Code Cache`, `GPUCache`, shader caches
- App updater `pending`, `AutoUpdate\Download`, update package folders

Tell the user these may be regenerated later.

## User Data

Require explicit user review:

- `%USERPROFILE%\Downloads`
- `%USERPROFILE%\Desktop`
- WeChat, QQ, WXWork, Tencent files
- Cloud-drive local data
- Game saves and screenshots
- WSL distributions
- Project folders

For chat apps, prefer built-in storage managers to preserve message database integrity.

## System Components

Do not manually remove:

- `C:\Windows\WinSxS`
- `C:\Windows\Installer`
- `C:\Windows\System32`
- `C:\Windows\servicing`
- `C:\pagefile.sys`
- `C:\swapfile.sys`
- `C:\System Volume Information`

Use Windows tools and settings only.

## WinSxS Handling

Analyze:

```powershell
Dism.exe /Online /Cleanup-Image /AnalyzeComponentStore
```

Clean:

```powershell
Dism.exe /Online /Cleanup-Image /StartComponentCleanup
```

If manual DISM fails with `0x80070005` / `STATUS_CANNOT_DELETE`:

1. Run the scheduled task:

```powershell
schtasks /Run /TN "\Microsoft\Windows\Servicing\StartComponentCleanup"
```

2. Reboot.
3. Re-run AnalyzeComponentStore.
4. If still recommended and failing, run:

```powershell
Dism.exe /Online /Cleanup-Image /RestoreHealth
sfc /scannow
```

Avoid `/ResetBase` unless the user accepts that installed updates cannot be uninstalled afterward.

## Pagefile Handling

Collect:

```powershell
Get-CimInstance Win32_PageFileUsage
Get-CimInstance Win32_ComputerSystem | Select-Object AutomaticManagedPagefile,TotalPhysicalMemory
```

Typical advice:

- For about 16 GB RAM and modest peak pagefile use, fixed 8192 MB on C is conservative.
- More aggressive: 4096-8192 MB.
- If another internal SSD exists, keep 2048-4096 MB on C and move the main pagefile to the other SSD.

Do not script pagefile changes by default. Give GUI steps unless the user specifically requests automation.
