# Release Checklist Template

This is the living template. Use a versioned `RELEASE_CHECKLIST_<version>.md` to record evidence for each release. Keep historical snapshots: `Test-PortablePackage.ps1` requires the current version's snapshot inside the portable zip. `AGENT.md` is the product authority.

## Scope and Safety

- [ ] State whether the release is advisory-only or includes the experimental cleanup action.
- [ ] Verify scanning and reporting are read-only and reports stay local (`privacy.uploaded == false`).
- [ ] Verify the frontend has no generic shell permission, arbitrary command execution, elevation helper, or network upload.
- [ ] If cleanup is included, verify plan-first confirmation, stored-report ID inputs, allowlist and execution-time rechecks, Recycle Bin-only behavior, conservative budget, and per-item audit logging.
- [ ] Confirm system-managed paths, chat stores, user data, installed applications, and reparse points cannot be cleaned.

## Automated Gates

- [ ] Run `npm ci` from a clean dependency install, then `npm run verify`; retain the complete output and exit code.
- [ ] Confirm the version contract across npm, Cargo, and Tauri.
- [ ] Confirm safety and scanner contract tests, seed-tree performance, lint, formatting, frontend coverage, build, npm audit, Rust tests/check, release build, packaging, checksum, and extracted scanner runtime smoke all passed.
- [ ] If cleanup is included, run the opt-in real Recycle Bin smoke with a disposable test file and verify it landed in the Recycle Bin.
- [ ] Verify both artifacts against `dist/checksums.txt` and inspect the portable zip contents.

## Manual Portable Runtime

- [ ] Extract the final zip into a fresh directory; launch its executable without a development server.
- [ ] Confirm Chinese UI, safety text, Quick scan default, start/cancel, a completed scan, Markdown/JSON opening, and latest-report reload after restart.
- [ ] Confirm system-managed items remain blocked, report history and comparison work, and the treemap shows report data only.
- [ ] If cleanup is included, inspect a real backend plan and rejection reasons, run only on controlled disposable data, and inspect outcomes and the audit log.
- [ ] Record OS version, locale, hardware, unsigned-build notice, limitations, and untested environments.

## Publication

- [ ] Prepare versioned release notes and a versioned checklist snapshot with actual evidence.
- [ ] Publish only the portable zip and checksum assets; the executable needs adjacent `_up_` resources.
- [ ] Preserve earlier release assets and records.
