# Security

## Reporting a vulnerability

Please report security problems privately, not in a public issue. Use GitHub's private vulnerability reporting
(**Security → Report a vulnerability**) on the project repository:
<https://github.com/vialian-dev/symvoulio/security/advisories/new>

Include:
- the version, from **About** in the app;
- your Windows version;
- steps to reproduce;
- what you expected.

Never include provider credentials, API keys or confidential documents in a report. We aim to acknowledge reports
within 7 days.

## Supported versions

| Version | Supported |
|---|---|
| 0.2.x | yes |

## What Symvoulio does with your data

- **No server.** Symvoulio has no backend of its own and sends no telemetry.
- **Documents** leave your computer only through the provider CLIs you installed and signed in to (Claude Code and
  Codex), under your accounts and the providers' terms. Each call receives the objective, the criteria, the original
  and current document text and your supporting files.
- **Credentials and diagnostics.** Sign-in uses the official CLI. Default live diagnostics contain controlled labels and counts. Persisted diagnostics contain metadata only. Session raw opt-in is not credential-safe.
- **Isolation flags.** Each role has a separate working folder. Claude receives `--strict-mcp-config` with an empty MCP config, `--tools ""`, `--disable-slash-commands`, and `--settings` containing `disableAllHooks: true` and `permissions.blockReadsOutsideWorkingDirectories: true`; the adapter sets `CLAUDE_CODE_DISABLE_CLAUDE_MDS=1` and `CLAUDE_CODE_DISABLE_AUTO_MEMORY=1`.
  Codex receives `--ignore-user-config`, `--ignore-rules`, `--ephemeral`, `--sandbox read-only`, and `--disable` for `shell_tool`, `unified_exec`, `hooks`, `plugins`, `apps`, `browser_use`, `computer_use` and `in_app_browser`.
  Evidence status: see R01 canary; these arguments alone do not establish real-provider isolation.
- **Review results.** Findings and revised documents are provider text by design. Inspect exported packages before sharing them.
- **Terminal input.** «Paste command» can only type single-line commands. Pasted text that contains a line break or a
  control character is refused, so a paste can never press Enter for you.

## Known risks

- The Windows installer is not code-signed. Verify the SHA-256 published with each release before installing.
- Anything you put in a document or criteria is sent to the providers. Do not review documents you are not allowed to
  share with them.

Default live diagnostics use controlled summaries. Review results (findings and the revised document) are provider text by design and are shown to the user. Raw provider output is an optional, memory-only session setting, off on app start; it is not credential-safe. The app never persists raw diagnostics. The separate isolation-canary tool writes raw evidence only to its chosen evidence directory.
