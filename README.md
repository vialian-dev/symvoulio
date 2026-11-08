# Symvoulio Review Demo v0.3

**Multi-provider document review with human approval.**

> No model grades its own work. Independent models verify. A human approves.

This repository contains the small public two-provider review demo. It does not contain the full Symvoulio Engine.

Symvoulio is an open-source desktop app for reviewing business documents with two different AI providers. One provider,
the **Author**, revises the document. A different provider, the **Independent Reviewer**, checks each revision against
your acceptance criteria. A simple, public rule decides whether another round is needed. You make the final decision.

It automates the copy-paste loop many people already run by hand between two AI chats. Both command-line sessions stay
visible, and every revision, finding and decision is kept.

Model agreement is evidence, not approval.

| | |
|---|---|
| Version | 0.3.1 (Windows and macOS) |
| Platforms | Windows 10 (1809 or newer) and Windows 11; macOS 13 Ventura or newer, one universal app for Apple Silicon and Intel (see [macOS](#macos)). Linux bash command pasting exists but is not measured. |
| Providers | Claude Code and OpenAI Codex, installed and signed in by you |
| License | MIT |

## Getting started (about 10 minutes)

### 1. Install the two provider CLIs and sign in

Symvoulio does not include the providers. On System check, click Install for a missing CLI. The app types the vendor's official installer command into that provider's visible terminal. Review it and press Enter yourself; the vendor installer runs there. The app never types Enter. Check again detects the installed version. Then sign in with your own accounts. Native installation needs no Node.js.

| Provider | Install (Windows PowerShell) | Account |
|---|---|---|
| Claude Code | `irm https://claude.ai/install.ps1 \| iex` | Pro, Max, Team, Enterprise or Console. The free claude.ai plan does not include Claude Code. |
| Codex | `powershell -ExecutionPolicy ByPass -c "irm https://chatgpt.com/codex/install.ps1 \| iex"` | ChatGPT sign-in, or an API key |

If native Codex installation is unavailable, `npm install -g @openai/codex` is a fallback that requires Node.js and npm.

Check them with `claude --version` and `codex --version`. Symvoulio's setup wizard can also start the sign-in for you:
it pastes `claude auth login` or `codex login` into the provider's panel, and you press Enter yourself.

#### Two separate CLI logins

Claude Code and Codex each have their own login, and you sign in to each one separately with your own account
(`claude auth login`, `codex login`). You sign in through the provider CLI; terminal text passes through Symvoulio memory.

### 2. Install Symvoulio

1. Download `Symvoulio_0.3.1_x64-setup.exe` and its `.sha256` file from the
   [Releases page](https://github.com/vialian-dev/symvoulio/releases/tag/v0.3.1)
2. Check the download in PowerShell. The hash must equal the one in the `.sha256` file:
   ```powershell
   (Get-FileHash .\Symvoulio_0.3.1_x64-setup.exe -Algorithm SHA256).Hash
   ```
3. Run the installer. It installs for the current user in `%LOCALAPPDATA%\Symvoulio`, needs no administrator rights,
   and adds a Start menu entry, a desktop shortcut and an uninstall entry.

#### Windows SmartScreen

The installer is **not code-signed**, so Windows warns before it runs:
- «Open File - Security Warning» (Publisher: Unknown Publisher): choose **Run**;
- or Microsoft Defender SmartScreen «Windows protected your PC»: choose **More info**, then **Run anyway**.

With **Smart App Control** turned on, Windows blocks unsigned apps and offers no bypass. See
[KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md).

On a Mac, install from the `.dmg` instead: see [macOS](#macos).

### 3. First launch

The setup wizard:
1. checks that both CLIs are installed, with their versions and whether you are signed in;
2. helps you connect each provider;
3. asks you to choose the Author (the other provider becomes the Reviewer);
4. asks for your default review policy: rounds, reviewer contribution, the new-text limit and the stop rules.

Symvoulio's connection settings and files store connection metadata, not provider credentials, and do not read the providers' credential files. Terminal input and output, including anything you type or paste during provider login, pass through the app's memory to and from the provider's CLI. Error-card diagnostics, logs and persisted diagnostics apply credential redaction. The terminal displays raw output through app memory; its replay backlog is never written to disk.

### 4. Your first review

1. **New review**: give it a name and an objective, and add acceptance criteria. Templates are available; you approve
   the criteria before the run.
2. Add the primary document (`.txt`, `.md`, `.docx` or a text-based `.pdf`) and optional supporting files. Try
   [samples/business_case_northwind.md](samples/business_case_northwind.md) with the criteria in
   [samples/sample-criteria.md](samples/sample-criteria.md).
3. **Start deliberation** and watch both panels. Each round runs like this:
   1. the Author revises and answers every open finding;
   2. the Reviewer reviews the revision against your criteria and the original;
   3. the gate decides whether to continue.
4. When the run stops, open **Human decision**: approve, reject or return the run for another round.
5. **Export** the package. Everything is also written to your workspace as the run goes.

## What you get

Each run writes a folder under `%USERPROFILE%\Symvoulio\runs\<run id>\output\` on Windows, and under
`~/Library/Application Support/Symvoulio/workspace/runs/<run id>/output/` on macOS:

```text
output/
  original/        your input files, byte for byte
  revisions/       revision_NN.docx and revision_NN.md
  reviews/         review_NN.json and author_response_NN.json
  final/           final_document.docx and .md, final_review.html, unresolved_issues.json, human_decision.json
  audit/           run_manifest.json, timeline.json, provider_output/
```

The JSON files follow the public schemas in [schemas/](schemas/). `run_manifest.json` records:
- the providers and their CLI versions;
- the SHA-256 of every input;
- your criteria and policy;
- the stop reason;
- your decision, bound to the revision you approved and that revision's SHA-256.

## How it works

- **Two separate local processes.** Symvoulio runs the official CLIs in their documented non-interactive modes:
  - Claude Code: `claude -p` with JSON output and a JSON schema;
  - Codex: `codex exec --json` with an output schema.
  Each role works in its own empty workspace folder.
- **Isolation.** Adapters request isolation through the flags listed in [SECURITY.md](SECURITY.md). Evidence status: see R01 canary. Reviewer prompt construction excludes author-response objects.
- **The gate** is public and simple:
  - it continues while Critical issues or unresolved High issues remain, while new material issues appear, while the
    minimum rounds are not done, or while a revision exceeds your new-text limit;
  - it stops when none of these apply, or at your maximum rounds;
  - no model ever approves; the decision is always yours.
- **Visible terminals.** Each provider panel is a real shell for sign-in and your own commands: on Windows PowerShell 7
  if present, otherwise Windows PowerShell; on macOS your login `zsh` (or `bash` as a login shell where zsh is missing). «Paste command» types a command without pressing Enter.
- **Local first.** No Symvoulio server exists. Your documents leave your computer only through the provider CLIs you
  signed in to, under your accounts and their terms.

## Build from source

Prerequisites:
- Rust (the version is pinned in `rust-toolchain.toml`);
- Node.js 22 with npm;
- on Windows, the Visual Studio C++ build tools and WebView2;
- on macOS, the Xcode Command Line Tools (`xcode-select --install`) and both Rust targets
  (`rustup target add aarch64-apple-darwin x86_64-apple-darwin`).

Use a filesystem that supports npm workspace links. Network shares that reject junctions or symlinks cannot link the local design system package.

```bash
npm ci
npm run tauri dev        # run the desktop app against the dev server
npm run tauri build      # build the app and the installer
```

Tests:

```bash
cargo test -p symvoulio-core -p symvoulio       # Rust: core and desktop commands (a fake provider CLI, no real calls)
npx playwright install chromium                 # once: the test browser (Linux: npx playwright install --with-deps chromium)
npm run build                                   # the browser suite serves the production build
npx playwright test --workers=1                 # UI in the browser (canned mode)
```

Opened in a plain browser (`npm run dev`), the UI runs in a canned demo mode with sample data. The desktop app uses the
real backend. [installers/windows/](installers/windows/) describes the cross-build of the Windows installer on Linux.
On a Mac, `sh installers/macos/build.sh` builds the universal `Symvoulio.app` and the `.dmg` (see [macOS](#macos)).

## macOS

Symvoulio 0.3 runs on macOS 13 Ventura or newer. One universal app runs natively on Apple Silicon and on Intel Macs.

### Install from the .dmg

1. Download `Symvoulio_0.3.1_universal.dmg` and its `.sha256` file from the Releases page.
2. Check the download in Terminal. The hash must equal the one in the `.sha256` file:
   ```bash
   shasum -a 256 ~/Downloads/Symvoulio_0.3.1_universal.dmg
   ```
3. Open the `.dmg` and drag **Symvoulio** onto the **Applications** shortcut next to it.
4. Eject the disk image, then start Symvoulio from **Applications**.

### First launch

The 0.3.1 `.dmg` and the app are signed with an Apple Developer ID certificate and notarized by Apple, so a download
whose SHA-256 matches the release page opens without the «Open Anyway» step. macOS may ask once whether to open an app
downloaded from the internet: click **Open**.

Builds you make yourself with `installers/macos/build.sh` and no Developer ID are signed ad hoc and not notarized.
macOS then refuses the first launch; allow it once, as Apple describes in
[Open a Mac app from an unknown developer](https://support.apple.com/guide/mac-help/open-a-mac-app-from-an-unknown-developer-mh40616/mac):
System Settings → Privacy & Security → **Open Anyway** next to the message about Symvoulio.

### Install the providers on macOS

On System check, click **Install** for a missing CLI: the app types the vendor's official command into that provider's
panel, and you press Enter yourself. The commands, from the vendors' pages (read 05.11.2026):

| Provider | Install (macOS zsh or bash) | Source |
|---|---|---|
| Claude Code | `curl -fsSL https://claude.ai/install.sh \| bash` | https://code.claude.com/docs/en/setup |
| Codex | `curl -fsSL https://chatgpt.com/codex/install.sh \| sh` | https://github.com/openai/codex |

Homebrew (`brew install --cask claude-code`, `brew install --cask codex`) and npm (`npm install -g @openai/codex`)
installations are found too. An app opened from Finder does not get your shell's PATH, so Symvoulio also looks in
`/opt/homebrew/bin`, `/usr/local/bin`, `~/.local/bin`, `~/.npm-global/bin`, the Codex package folder and your login
shell's PATH. Then, on «Connect Claude Code» and «Connect Codex», click **Sign in**: the app runs the official login
(`claude auth login`, `codex login`) in that provider's panel, and you finish the sign-in in your browser.

### Where Symvoulio keeps its data on macOS

Settings and runs live in `~/Library/Application Support/Symvoulio/` (`settings.json` and `workspace/`). To remove
everything, quit Symvoulio, delete `Symvoulio.app` from Applications and delete that folder.

## Statements

**Provider CLI Independence.** Symvoulio does not embed, proxy, resell, or provide access to Anthropic Claude Code or
OpenAI Codex. Claude Code and Codex are separately installed command-line applications. Users authenticate directly
with each provider through the provider's official CLI login flow. Symvoulio orchestrates local commands authorized by
the user and exchanges user-selected files between isolated local workspaces. The application does not store provider passwords, browser cookies, or authentication tokens in its settings or files, and does not read the providers' credential files. Terminal input, including anything typed or pasted during provider login, passes through the app's memory to the provider's CLI.

**Account Responsibility.** Users are responsible for maintaining valid provider accounts, subscriptions, permissions,
and compliance with applicable provider terms and usage limits.

**Human Accountability.** Model agreement is not approval. A human remains responsible for selecting sources, defining
criteria, resolving disagreements, accepting the final version, and deciding whether the output is suitable for use.

**Trademark Notice.** Symvoulio is not affiliated with or endorsed by Anthropic or OpenAI. Product and company names
are trademarks of their respective owners.

## Feedback

Symvoulio Review Demo was designed, built and verified by the Mensurabilitas Automatic Coding Pipeline (MACP). It is a v0.x release: we ship fast and fix fast. If something doesn't behave as described, please open a GitHub Issue — reported bugs get fixed.

## More

- [KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md): what v0.3 does not do
- [SECURITY.md](SECURITY.md): reporting a vulnerability, and what the app does with your data
- [CHANGELOG.md](CHANGELOG.md)
- [CONTRIBUTING.md](CONTRIBUTING.md)

## License

MIT. Copyright (c) 2026 Vialian. See [LICENSE](LICENSE).
