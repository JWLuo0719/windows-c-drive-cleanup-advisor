# Codex project guidance

Read [AGENT.md](AGENT.md) before changes: it is the maintained source for this advisor's read-only scan/report boundary, experimental cleanup allowlist, commands, and release gates. This discovery entry does not create a second product authority.

Load the root SKILL.md only for scanner heuristics or cleanup-report reasoning. Use Tauri guidance only for IPC/capabilities or native packaging work; a UI text correction does not require a generic workflow coordinator.

Done means the requested behavior or artifact is delivered and the relevant documented safety/scanner/frontend/Rust checks pass. Release work still requires the clean-install full `npm run verify` gate and actual portable runtime evidence; source builds are not release proof. No cleanup, deletion or settings changes are authorized by a documentation task.

When preparing a task prompt, specify the scan/report behavior, allowed paths, read-only boundary, any explicitly scoped cleanup action, expected artifact, and the applicable checks from AGENT.md. Preserve actual command outputs in the existing release/status documentation and identify untested environments.
