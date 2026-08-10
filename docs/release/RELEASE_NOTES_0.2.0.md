# Release Notes: Windows C Drive Cleanup Advisor 0.2.0

Release date: 2026-08-12

## Summary

Windows C Drive Cleanup Advisor 0.2.0 is a read-only Windows desktop advisor for diagnosing C drive disk pressure. It scans local paths, skips reparse points, creates local Markdown and JSON reports, and ranks candidates for human review.

This release does not delete, move, uninstall, upload, collect telemetry, or change Windows settings.

## What Changed

- Repeated low-risk cache files under a common cache directory are aggregated into one directory-level review candidate.
- Existing scanner-provided cache directories suppress duplicate child-file rows.
- Specific review candidates continue to rank ahead of broad drive-root summaries.
- Portable packaging now derives its artifact version from Tauri configuration and validates version agreement across Tauri, npm, and Cargo.
- Verification now fails on any non-zero npm, Cargo, or child PowerShell exit code.
- The package includes an MIT license and release-facing documentation.

## Safety and Privacy

- The frontend has no generic shell permission and cannot execute arbitrary commands.
- Rust launches only the bundled scanner with fixed PowerShell arguments.
- Cache candidates remain manual-review only: `cleanable` is always `false`.
- System-managed paths remain blocked guidance, never cleanup tasks.
- Reports stay on the local computer; `privacy.uploaded` remains `false`.

## Compatibility and Distribution

- Supported release target: Windows 10 or Windows 11 x64.
- The release is a portable zip. Extract the whole archive before launching it; the executable requires the bundled `_up_` scanner resources.
- The binary is unsigned. Microsoft Defender SmartScreen may display a warning.

## Verification

Run:

```powershell
npm run verify
```

The release gate covers safety, scanner contract, frontend tests and build, dependency audit, Rust tests and check, Tauri release build, portable packaging, SHA-256 generation, and an extracted portable runtime scan.

## Release Assets

- `windows-c-drive-cleanup-advisor-0.2.0-windows-x64.zip`
- `checksums.txt`

Verify the downloaded archive before extracting it:

```powershell
Get-FileHash .\windows-c-drive-cleanup-advisor-0.2.0-windows-x64.zip -Algorithm SHA256
```
