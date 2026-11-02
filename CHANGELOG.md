# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
the version numbers follow [Semantic Versioning](https://semver.org/).

## [0.2.0] — unreleased

### Added
- Setup can paste official Claude Code and Codex installer commands for Windows PowerShell, macOS zsh and Linux bash. You press Enter to run them.
- Installer completion diagnostics and still-running manual-check guidance after 15 seconds without a prompt marker; the guard and attribution remain until completion.
- Detection searches native installer directories; new Windows panels refresh machine and user PATH, and existing panels use resolved login executables without restarting the app.

### Fixed
- Preserve shell startup files and prompt hooks, provide protection against blind marker collisions (the token is available to child processes), and handle rejected login pastes.

## [0.1.0] — Symvoulio Review Demo v0.1 for Windows (tag `v0.1.0-windows`) — unreleased

The first public release, for Windows 10 (1809 or newer) and Windows 11.

### Added
- **Desktop app** (Tauri 2, Rust and React).
  - Two provider panels, Claude Code and Codex, always visible. Each is a real shell (PowerShell 7 or Windows
    PowerShell) with Readable and Raw views of the run.
- **Setup wizard.**
  - Detects both CLIs, their versions and sign-in state.
  - Guides the official sign-in (`claude auth login`, `codex login`) without ever pressing Enter for you.
  - Lets you choose the roles: the Author and a different Independent Reviewer.
  - Sets the default review policy.
- **Reviews.**
  - Objective, acceptance criteria (with templates) that you approve before the run, a primary document (`.txt`,
    `.md`, `.docx`, text `.pdf`) and supporting files (also `.csv`).
  - Policy per review: 1–5 rounds, minimum rounds, reviewer contribution (Verify, Correct, Improve), a new-text limit
    and stop rules.
- **Round loop.**
  - The Author revises and answers each open finding as accepted, rejected with evidence, partially accepted or
    needing a human decision.
  - The Reviewer checks the revision against the criteria and the original.
  - A public, deterministic gate decides whether to continue.
  - Pause, Resume and Stop. Completed rounds survive a crash and are recovered on restart.
- **Human decision.** Approve, reject or return the run for another round, up to the run's maximum of rounds. The
  decision records the approved revision and its SHA-256. «This is not an approval» stays visible until you decide.
- **Issue register** with the history of each finding, the author's answers, the author's questions to a human, and
  human decisions per issue.
- **Document views**: Original, Current, Diff (side by side or inline, including deleted sections) and Final.
- **Export package** (`output/`): original, revisions, reviews, final document (`.docx`, `.md`), `final_review.html`,
  `unresolved_issues.json`, `human_decision.json`, `run_manifest.json`, `timeline.json` and the provider output.
- **Public JSON schemas** for the author response, the review and the run manifest.
- **Safety.**
  - Provider calls request isolation controls: Claude receives `--tools ""`, `--strict-mcp-config` with an empty MCP config,
    `--disable-slash-commands`, and `--settings` with `disableAllHooks: true` and `permissions.blockReadsOutsideWorkingDirectories: true`;
    Codex receives `--ignore-user-config`, `--ignore-rules`, `--ephemeral`, `--sandbox read-only` and feature-disable flags (see SECURITY.md).
    The retained canary runs measured 0 external tool executions; two Claude `StructuredOutput` calls are intentionally excluded.
    Claude initialization advertised three built-in plugins; requested controls and these runs do not establish universal isolation.
  - Provider answers are validated against the schemas and checked for consistency.
  - Credential redaction applies to provider diagnostics/logs; documents are stored as given, including original document bytes.
    Inspect exported packages before sharing them.
  - Pasted text with line breaks is refused in the terminals.
- **Windows installer** (NSIS, per user, no administrator rights).

### Known limitations
See [KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md).
