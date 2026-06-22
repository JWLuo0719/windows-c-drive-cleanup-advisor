# Windows C Drive Cleanup Advisor

This package is a read-only C drive scanner and cleanup advisor. It does not delete, move, uninstall, or change settings.

## Desktop App

From source:

```powershell
npm install
npm run tauri:dev
```

The desktop app uses a Rust IPC boundary. The frontend does not receive generic shell permissions; Rust runs the bundled PowerShell scanner with fixed arguments.

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

Run the read-only safety boundary check:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\Test-SafetyBoundary.ps1
```

For release preparation, also review:

```text
docs\RELEASE_CHECKLIST.md
```

## Verify The Project

Run the full local verification pipeline:

```powershell
npm run verify
```

This includes the read-only safety boundary check, frontend tests, Rust tests, release build, portable package validation, and checksum generation.

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

## Output

Reports are written to:

```text
report\
```

The Markdown report lists:

- current C drive pressure;
- largest real local folders, excluding reparse points;
- common cache and app-data drilldowns;
- large files;
- pagefile and hibernation state;
- WinSxS analysis when elevated;
- cleanup advice for the user to decide on.

## Safety

The tool is advisory only. Users should decide what to clean. Chat app data, system components, WSL distributions, and installed programs need extra care.
