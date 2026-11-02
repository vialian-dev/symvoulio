#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Vialian
# Produce the selected public tree of a commit (default HEAD), omitting private comment metadata, then check it. With explicit --worktree,
# copy the listed paths' working files from the selected paths instead (for checking uncommitted edits).
# Push the result to GitHub as ONE new commit on an orphan branch: never push the private history. In the clone that
# pushes to GitHub, install the guard first: cp tools/public-export/pre-push .git/hooks/pre-push (it re-runs check.sh
# on every pushed commit and also refuses unfilled publication values).
# usage: tools/public-export/export.sh <out dir> [commit|--worktree]
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd); root=$(git -C "$here" rev-parse --show-toplevel)
out=${1:?usage: export.sh <out dir> [commit]}; rev=${2:-HEAD}
[ -e "$out" ] && { echo "refusing: $out exists"; exit 2; }
mapfile -t want < <(grep -vE '^\s*(#|$)' "$here/include.txt")
paths=(); for p in "${want[@]}"; do
  if { [ "$rev" = "--worktree" ] && [ -e "$root/$p" ]; } || git -C "$root" cat-file -e "$rev:$p" 2>/dev/null; then paths+=("$p"); else echo "MISSING in $rev: $p"; missing=1; fi
done
mkdir -p "$out"
if [ "$rev" = "--worktree" ]; then
  (cd "$root" && git ls-files -z --cached -- "${paths[@]}" | xargs -0 -I{} cp --parents {} "$out")
  echo "exported working tree: $(find "$out" -type f | wc -l) files"
else
  git -C "$root" archive --format=tar "$rev" -- "${paths[@]}" | tar -x -C "$out"
  echo "exported $(git -C "$root" rev-parse --short "$rev"): $(find "$out" -type f | wc -l) files"
fi
# Private review harnesses require internal build hosts; keep public checks independent.
rm -f "$out/tests/ct_publication3.check.ts" "$out/tests/ct_publication4.check.ts"
node "$here/sanitize-comments.mjs" "$out"
"$here/check.sh" "$out"
[ -z "${missing:-}" ] || { echo "export incomplete: listed paths missing"; exit 1; }
