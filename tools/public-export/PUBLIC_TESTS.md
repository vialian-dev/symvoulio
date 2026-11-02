// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Vialian

Public source validation

From a fresh repository containing the exported tree (the lockfile includes the design system peers; no peer-resolution flags are required):

Use a filesystem that supports npm workspace links. Network shares that reject junctions or symlinks cannot link the local design system package.

- `npm ci`
- `npm run typecheck`
- `npm run build`
- `npx playwright install chromium` (once; on Linux `npx playwright install --with-deps chromium`)
- `npm test` (after `npm run build`: the browser suite serves the production build)
- `npx jiti tests/ct_publication.check.ts`
- `npx jiti src/services/wire.check.ts`
- `cargo test -p symvoulio-core`
- `cargo clippy -p symvoulio-core --all-targets -- -D warnings`
- On Windows: `cargo test -p symvoulio --test command_layer` and `cargo clippy -p symvoulio --all-targets -- -D warnings`.
- For desktop tests, follow `playwright.desktop.config.ts` and its setup file.

The internal combined publication/canary harness `tests/ct_publication3.check.ts` and `tests/ct_publication4.check.ts` are excluded from this export. The public publication check and isolation evidence tool are included. Running the real canary requires signed-in provider CLIs and spends provider quota; `bash tools/isolation-canary/canary.sh <evidence-directory>` checks both role assignments. Its positive detector controls run automatically. A passing fixture run validates the detector, not provider isolation on a real platform.

`bash tools/public-export/export.sh <new-directory>` exports the committed HEAD tree by default. Run `npm ci` in the source checkout first: export uses the installed jiti TypeScript parser to omit private review metadata comments from selected test files and verifies that executable code is unchanged. Assertions, titles, and string contents are preserved, and the public scanner remains unchanged. Pass a commit to select another snapshot; explicit --worktree exports tracked working files for local review. Create a new root commit in the exported directory; do not reuse the source repository history. Install `tools/public-export/pre-push` to scan every newly reachable commit before publishing. Run `bash tools/public-export/check.sh .` to scan the working tree.

For a fresh export and desktop discovery check, use a new child of a temporary directory (the export destination itself must not exist), and stop if any command fails:

```bash
export_parent=$(mktemp -d)
bash tools/public-export/export.sh "$export_parent/export" &&
  cd "$export_parent/export" &&
  npm ci &&
  npx playwright test -c playwright.desktop.config.ts --list &&
  npx playwright test -c playwright.desktop.r07b.config.ts --list
```

Passing the existing directory returned by `mktemp -d` directly to the exporter exits 2. Do not continue to npm or Playwright after that failure: there is no exported configuration to discover. Install with `npm ci` before discovery so `npx` uses the lockfile's Playwright rather than downloading an unrelated version.

Run `bash tools/public-export/history-check.sh <export-directory>` to create a fresh orphan repository with two commits and scan every reachable commit using the pre-push guard. `--positive-control` plants a forbidden value in the first commit and removes it in the second; the scan must fail despite the clean tip. Run `npx jiti tools/public-export/rework.check.ts` for the export and history regressions.
