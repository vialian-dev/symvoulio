# Known limitations: Symvoulio v0.3 (Windows and macOS)

Symvoulio helps you review most text-based business documents. It does not promise support for every document.

## Documents
- Primary documents: `.txt`, `.md`, `.docx` and text-based `.pdf`. Supporting files can also be `.csv`.
- Not supported in v0.3: scanned PDFs and OCR, PowerPoint, spreadsheet formula validation, VBA or macros, complex
  legal redlining, images inside documents, and exact preservation of advanced Word layout. A PDF with no text layer
  is refused with a clear message.
- Revisions are written as Markdown and as a simple `.docx` (headings, paragraphs, lists, bold and italic). The
  original document is kept byte for byte in `output/original/`.
- Extracted primary and supporting text together must fit within 8 MB at start. Every complete UTF-8 provider prompt (including criteria, findings and revisions) is capped at 9 MB; exceeding it pauses the run and names the largest inputs.

## Review
- Model agreement is evidence, not approval. A human makes every final decision; the app never sets «Approved» by
  itself.
- «Maximum new content» is measured as an observable proxy: the share of words added by a revision. It is not a value
  measure.
- The gate is the simple public rule of the specification. A finding counts as *material* when its severity is
  critical, high or medium.
- The app passes no model choice to the Codex CLI and Codex's JSON output does not report the model, so
  `run_manifest.json` records the reviewer model as `null` when Codex is the reviewer.

## Providers
- S1 Install types the vendor's official installer command into the visible provider terminal. The user presses Enter; the vendor installer runs there. The app never types Enter. Pending input, unknown editor state (including history recall), or a busy panel prevents another install paste. Clear recalled/edited input with Ctrl+C before retrying; timeout alone never proves the shell is idle. After 15 seconds without a completion marker, manual-check guidance appears while the panel stays locked and attribution remains until a real marker. A completed installer command with a recognized marker triggers a version probe; exit zero alone does not prove installation. Check again also probes without restarting the app. If PATH still hides the CLI, check the vendor's PATH instructions and retry.
- You need your own installed and signed-in Claude Code and Codex CLIs. Symvoulio's connection settings and files store connection metadata, not provider credentials, and do not read the providers' credential files. Terminal input and output, including anything you type or paste during provider login, pass through the app's memory to and from the provider's CLI. Error-card diagnostics, logs and persisted diagnostics apply credential redaction. The terminal displays raw output through app memory; its replay backlog is never written to disk.
- An `ANTHROPIC_API_KEY` in your environment takes precedence over your Claude subscription in non-interactive calls,
  as Claude Code's documentation states.

## Windows
- The installer is not code-signed. Measured on 30.10.2026 (Windows 11, Smart App Control Off): a downloaded copy
  started from another program showed «Open File - Security Warning» («The publisher could not be verified»,
  Publisher: Unknown Publisher) with **Run** and **Cancel**. Choose Run. When you start a downloaded copy by
  double-clicking it in Explorer, Microsoft Defender SmartScreen may show «Windows protected your PC» instead; choose
  «More info» and then «Run anyway». That dialog is not yet measured on our test machine.
- With **Smart App Control** set to On, Windows blocks unsigned apps and offers no per-app bypass. This follows
  Microsoft's documentation; we did not test it, because switching Smart App Control on cannot be undone easily.
- The installer installs per user in `%LOCALAPPDATA%\Symvoulio` (about 21 MB), needs no administrator rights, and
  adds a Start menu entry, a desktop shortcut and an entry in Installed apps for uninstalling.
- **Workspace under OneDrive-synced folders may fail.** The default workspace is `%USERPROFILE%\Symvoulio`. In our
  test a run under `OneDrive\Documents` stopped with «Access is denied» while writing a provider file. The same run in
  a plain folder completed. The root cause is not yet measured.
- Requires Windows 10 1809 or newer (ConPTY) with WebView2, which Windows 11 includes.

## macOS

Measured on one Mac: MacBook Pro M5 (Apple Silicon), macOS 27.0.1 — over SSH on 05.11.2026, and on 08.11.2026 on
the screen (0.3.1, notarized build) with real Claude Code and Codex sign-ins.

- **Notarized release build only.** The 0.3.1 `.dmg` and app on the release page are signed with an Apple Developer
  ID and notarized. A build you make yourself without a Developer ID is signed ad hoc, and macOS blocks its first
  launch until you choose System Settings → Privacy & Security → Open Anyway. The About screen of 0.3.1 still
  describes that ad hoc case («not notarized by Apple») also in the notarized build; the text is wrong, the app is not.
- **Measured on macOS 27.0.1 only.** The app declares macOS 13 Ventura as its minimum; versions 13 to 26 are not
  measured.
- **Intel Macs not measured on hardware.** The app is one universal build (`x86_64` and `arm64`). Its Rust tests
  pass as `x86_64` under Rosetta 2 on the Apple Silicon Mac; no Intel Mac was available.
- **One live sample review on a Mac.** On 08.11.2026 a short review of the sample document ran with both providers
  signed in (both answered, Stop worked). Longer runs, export and the human decision screen on a Mac are covered by the
  shared tests with stand-in CLIs, not yet by a measured run.
- **Provider CLIs in other folders.** An app opened from Finder gets no shell PATH. Symvoulio looks in the PATH it
  was started with, your login shell's PATH (read again when the last read is older than 30 seconds, at most 5 seconds
  each), `/opt/homebrew/bin`, `/usr/local/bin`, `~/.local/bin`, `~/.npm-global/bin`, `~/.volta/bin`, `~/.bun/bin`,
  nvm's version folders and the Codex package folder (`~/.codex/packages/standalone/current/bin`, `CODEX_HOME`,
  `CODEX_INSTALL_DIR`). A CLI elsewhere is not found.
- **bash panels** (only where zsh is missing) read the login files like `bash -l` but are not started with `-l`, so
  `shopt login_shell` reports off.
- **Data folder.** Settings and runs are kept in `~/Library/Application Support/Symvoulio/`, which Finder hides by
  default; open it from the Export screen or with Finder → Go → Go to Folder.
- **Bundle identifier.** The app keeps the identifier `com.symvoulio.app` of the Windows version. Tauri warns that an
  identifier ending in `.app` is not recommended on macOS; changing it would move the Windows settings folder.

## Known small issues in 0.3.1

Seen on the Mac on 08.11.2026; they do not stop a review.

- **Run setup again keeps you in the wizard.** During a repeated setup, clicking Reviews in the left rail returns to
  step 1 (System check); you leave the wizard only through Finish setup. The rail highlights Settings meanwhile.
- **Stop and the Reviewer panel can disagree.** After Stop the run may say that the last revision's review did not
  finish, while the Reviewer's panel shows that review as completed. The cause is not yet established.
- **Settings → Language says «English only in v0.2.»** The text is out of date; the app is English only.
- **The Author panel shows terse lines** such as «system: provider event» and «assistant: provider event» instead of
  readable progress.
- **«Open sample review» opens a finished demonstration review**, not a new run. For a live run use New review → «Use
  the sample document»; the review name and objective are not filled in for you.

## Not in v0.3
- The `.msi` installer, a measured Linux version, and providers other than Claude Code and Codex.

The credential-safety guarantee covers the live diagnostics stream only. Review results (findings and the revised document) are provider text by design and are shown to the user. Raw provider output is an optional, memory-only session setting, off on app start; it is not credential-safe. Raw diagnostics are never persisted.

## Desktop test readiness

The desktop navigation failure was caused by the test harness accepting a WebView2 error page as ready. The harness
now requires the app origin and the mounted app. There is no product defect.
