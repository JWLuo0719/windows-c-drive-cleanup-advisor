# GUI Smoke Test Report Template

Use this template after launching the release executable on Windows. Keep the test read-only: do not delete, move, uninstall, upload, or change settings.

## Build Under Test

- Version:
- Date:
- Tester:
- Windows version:
- Release executable path:
- Portable zip path:
- SHA-256 checked against `dist\checksums.txt`: yes/no

## Preflight

- [ ] `npm run verify` passed before this smoke test.
- [ ] Release executable launches without a white screen.
- [ ] UI is Chinese.
- [ ] Safety ledger is visible.
- [ ] Quick scan is selected by default.
- [ ] No generic shell prompt, cleanup action, delete action, upload action, or telemetry prompt is visible.

## Quick Scan Waiting Experience

- Scan ID:
- Start time:
- End time:
- Approximate duration:

Checklist:

- [ ] Progress starts and status messages update.
- [ ] Scan companion panel appears while scanning.
- [ ] Companion panel shows elapsed time.
- [ ] Companion panel shows current-stage time.
- [ ] Companion panel shows current focus.
- [ ] Companion panel explains why the current stage may be slow.
- [ ] Companion panel shows the likely next step.
- [ ] Activity feed records recent scan messages.
- [ ] During top-root sizing, heartbeat messages appear instead of looking frozen around 18%.
- [ ] During a long unchanged stage, activity feed adds an explanatory "current stage is still working" update.
- [ ] During large-file enumeration, heartbeat messages appear instead of looking frozen around 65%.
- [ ] Cancel button remains available while scanning.

Notes:

```text

```

## Cancel Path

- Scan ID:

Checklist:

- [ ] Cancel a running scan.
- [ ] UI moves to a cancelled state.
- [ ] No source file or folder is changed by the app.
- [ ] A later stale progress update does not overwrite the cancelled state.

Notes:

```text

```

## Completed Report Path

- Scan ID:
- Markdown report path:
- JSON report path:

Checklist:

- [ ] Completed scan loads results automatically.
- [ ] After app restart, **载入最近报告** restores the latest local report without running a new scan.
- [ ] Markdown report opens from the UI.
- [ ] JSON report opens from the UI.
- [ ] Report folder opens from the UI.
- [ ] Copy report paths includes both Markdown and JSON paths.
- [ ] JSON has `privacy.uploaded` set to `false`.
- [ ] Result guide is visible.
- [ ] Result health panel reports local-only privacy status.
- [ ] Result health panel summarizes recommendation count.
- [ ] Result health panel reports unreadable paths when applicable.
- [ ] Result health panel reports skipped reparse points.
- [ ] Unreadable paths are grouped by protected area instead of shown as a long raw list.
- [ ] Recommendation list prioritizes specific review candidates ahead of broad root-folder summaries.
- [ ] Low-risk cache guidance remains manual review only.
- [ ] System-managed or protected Windows paths are marked as blocked guidance, not cleanup tasks.

Notes:

```text

```

## Deep Scan Mode

- Scan ID:

Checklist:

- [ ] Switching to Deep scan changes the mode description.
- [ ] Deep scan starts with common-root drilldowns enabled.
- [ ] Deep scan still remains read-only.

Notes:

```text

```

## Decision

- [ ] Pass: release is ready for the next packaging/publication step.
- [ ] Hold: release needs changes before publication.

Blocking issues:

```text

```

Follow-up polish:

```text

```
