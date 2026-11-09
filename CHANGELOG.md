# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
the version numbers follow [Semantic Versioning](https://semver.org/).

## [0.3.2] — 2026-11-08 — small fixes

### Fixed
- «Run setup again» no longer traps you in the wizard: the left rail leaves it, and your settings are kept.
- After Stop, the provider panel says that an answer which arrived too late is not used, so the panel and the run
  agree (the run discards an answer that was cut by Stop).
- Settings → Language no longer mentions an old version.
- Provider panel lines say what is happening (session started, thinking, reasoning, running a command, writing the
  structured answer, …) instead of «provider event».
- The Reviews button that opens the finished demonstration review is now called «View finished sample review»;
  a live run of the sample starts from New review.
- On macOS, About no longer says the app is not notarized: the release is signed with an Apple Developer ID and
  notarized; the Open Anyway steps are only for builds you make yourself without a Developer ID.

Windows and macOS behaviour is otherwise unchanged.

## [0.3.1] — 2026-11-08 — macOS support and fixes

The first public release for macOS (it includes 0.3.0, which was not released on its own), with fixes from the first
look at 0.3.0 on a Mac. Windows behaviour is unchanged.

### Added
- **Signed and notarized macOS release.** The `Symvoulio_0.3.1_universal.dmg` on the release page is signed with an
  Apple Developer ID certificate and notarized by Apple, so it opens without System Settings → Open Anyway.

### Changed
- **Sign in from the app.** On «Connect Claude Code» and «Connect Codex», «Sign in» runs the provider's official
  login (`claude auth login`, `codex login`) in its panel; you only finish the sign-in in the browser. The page turns
  to «connected» by itself, and the panel is then cleared. «Paste command» still types the command without running it.
- **Help at the bottom of the left rail.** On the first three setup steps the rail says what to do now, for the
  current step and provider state. The picture placeholders in the step cards are gone.
- **Provider panels on macOS start in the app's workspace** (`~/Library/Application Support/Symvoulio/workspace`),
  not in your home folder.

### Fixed
- A CLI installed while the app is open is found by «Check again»: the login shell's PATH is read again after 30 s,
  and the Codex standalone package folder (`~/.codex/packages/standalone/current/bin`, `CODEX_HOME`,
  `CODEX_INSTALL_DIR`) is searched. A panel opened before the install types the CLI's full path.
- The panels are cleared after the sign-in and when setup is finished: no «Login successful. Press Enter», no install
  transcript, no «Terminal busy or closed» while a sign-in waits for the browser.
- «Answered by Your name»: the missing space.
- Settings on macOS no longer show PowerShell text.
- The Reviews list shows the whole date and time on macOS («Nov 7, 23:04», was cut to «Nov 7 at 23:…»).

## [0.3.0] — macOS — not released on its own (included in 0.3.1)

Symvoulio for macOS 13 Ventura or newer, as one universal app for Apple Silicon and Intel. Windows behaviour is
unchanged.

### Added
- **macOS app and installer.** `installers/macos/build.sh` builds a universal `Symvoulio.app` and
  `Symvoulio_0.3.0_universal.dmg` with an Applications shortcut, and writes the `.dmg`'s SHA-256 next to it.
- **Signing.** The app is signed ad hoc and is not notarized: macOS asks you to allow the first launch in
  System Settings → Privacy & Security → Open Anyway (README, «macOS»). With an Apple Developer ID identity and the
  App Store Connect variables `APPLE_API_ISSUER`, `APPLE_API_KEY` and `APPLE_API_KEY_PATH`, the same script signs and
  notarizes the app and the `.dmg`. Keys are read only from files named by environment variables, never printed.
- **Provider panels on macOS** open your login `zsh`; without zsh, `bash` reading the login files as `bash -l` does.
- **About** explains the macOS first launch on a Mac instead of Windows SmartScreen.

### Fixed
- An app opened from Finder gets no shell PATH on macOS. Detection and provider calls now also search the login
  shell's PATH, `/opt/homebrew/bin`, `/usr/local/bin`, `~/.local/bin`, `~/.npm-global/bin`, `~/.volta/bin`,
  `~/.bun/bin` and nvm folders, and provider processes get that PATH, so npm-installed CLIs find `node`.
- Install refused a macOS `bash` panel; it now pastes the same official command as in zsh.
- On macOS, settings and runs are kept in `~/Library/Application Support/Symvoulio/` instead of
  `~/Library/Application Support/com.symvoulio.app/` and `~/Symvoulio`.
- Version numbers in Cargo, Tauri and npm agree: 0.3.0.

## [0.2.0] — 2026-11-02

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
