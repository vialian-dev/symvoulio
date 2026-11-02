# Contributing

Thank you for helping. Issues and pull requests are welcome on the project repository:
<https://github.com/vialian-dev/symvoulio>

## Before you open an issue

- Describe the Windows version, the Symvoulio version (shown in **About**) and the versions of `claude` and `codex`.
- Never paste credentials, API keys, or confidential documents. Run packages can contain document text: share only
  what you may share.
- Report security problems privately, as described in [SECURITY.md](SECURITY.md).

## Development

Prerequisites:
- Rust (the version is pinned in `rust-toolchain.toml`);
- Node.js 22 with npm;
- on Windows, the Visual Studio C++ build tools and WebView2.

```bash
npm ci --legacy-peer-deps
npm run dev              # the UI in a browser, canned demo mode
npm run tauri dev        # the desktop app with the real backend
```

Layout:
- `crates/symvoulio-core`: the review engine. It holds the schemas as Rust types, the issue ledger, the public gate,
  the provider adapters, document extraction and the output package, and has no Tauri dependency.
- `src-tauri`: the desktop shell, the Tauri commands and the terminal panels.
- `src`: the React UI.
  - `src/services` holds the adapter between the browser's canned mode and the desktop backend.
- `schemas`: the public JSON contracts.

## Tests

Every change needs a test. A bug fix needs a test that fails without the fix.

```bash
cargo test -p symvoulio-core -p symvoulio
cargo clippy -p symvoulio-core -p symvoulio --all-targets -- -D warnings
npx tsc -p tsconfig.app.json --noEmit
npx playwright test --workers=1
```

The Rust tests never call a real provider. They use `fake-cli`, a small stand-in for both CLIs that prints recorded
answers shaped like the real ones.

## Style

- Code, comments, strings and commit messages in English.
- Every source file starts with:
  ```
  // SPDX-License-Identifier: MIT
  // Copyright (c) 2026 Vialian
  ```
- Every UI string lives in `src/i18n/en.json`.
- Keep the public rules public and simple: the gate and the schemas are part of the product's promise.

By contributing you agree that your contribution is licensed under the MIT License.
