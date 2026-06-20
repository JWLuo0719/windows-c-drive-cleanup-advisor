# Windows C Drive Cleanup Advisor

A read-only Windows desktop advisor for diagnosing C drive disk pressure. It scans real local paths, skips reparse-point locations, ranks large folders and files, and explains what is safe to review.

## Current Status

Version `0.1.0` is intentionally advisory only:

- Tauri v2 desktop shell with React/Vite UI.
- Rust owns the narrow IPC boundary.
- The bundled PowerShell scanner runs with fixed arguments from Rust.
- Markdown and enriched JSON reports are written locally.
- No cleanup, deletion, move, uninstall, upload, or settings change is performed.

## Run From Source

```powershell
npm install
npm run tauri:dev
```

The legacy script runner is still available:

```powershell
.\Run-CDriveCleanupAdvisor.cmd
```

## Safety Boundary

The app is built around a conservative rule: diagnose first, let the user decide. The frontend does not receive generic shell permissions and cannot invoke arbitrary commands. The Rust backend only exposes fixed scan commands.

## What This App Never Deletes

Version `0.1.0` never deletes anything. It also never automatically handles:

- `C:\Windows\WinSxS`
- `C:\Windows\Installer`
- `C:\Windows\System32`
- `C:\System Volume Information`
- `C:\pagefile.sys`, `C:\swapfile.sys`, `C:\hiberfil.sys`
- WSL `ext4.vhdx` files
- WeChat, QQ, or WXWork message stores
- Installed application directories

## Privacy

Reports are written to local disk only. The app does not upload scan output, file paths, telemetry, or usage data.

## Known False Positives

Heuristic categories are advisory. Some cache-looking folders can contain important app state, and some app-managed folders can contain disposable installers. Review paths before acting.

## Roadmap

- `v0.1`: Read-only GUI, risk groups, Markdown/JSON reports.
- `v0.2`: Better progress, cancellation polish, export flow, mock tests.
- `v0.3`: Experimental low-risk cache cleanup allowlist, using `reportId + candidateIds` only and moving items to the recycle bin by default.
- `v1.0`: Signed or clearly unsigned release, installer, portable zip, checksums, complete release verification guide.

## SmartScreen Notice

Unsigned Windows builds can trigger Microsoft Defender SmartScreen warnings. Public releases should either be signed or clearly marked as unsigned.

## Checksums

Release builds should publish SHA-256 checksums next to installers and portable archives.

## How To Verify The Release

```powershell
Get-FileHash .\Windows-C-Drive-Cleanup-Advisor-0.1.0.zip -Algorithm SHA256
```

Compare the result with the checksum published in the GitHub Release.
