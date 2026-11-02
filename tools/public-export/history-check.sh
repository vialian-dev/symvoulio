#!/usr/bin/env bash
# // SPDX-License-Identifier: MIT
# // Copyright (c) 2026 Vialian
# Validate a fresh public history. --positive-control plants a leak only in its first commit.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
source=$(cd "${1:?usage: history-check.sh <export dir> [--positive-control]}" && pwd)
mode=${2:-}
[[ -z "$mode" || "$mode" = --positive-control ]] || exit 2
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir "$tmp/repo"
cp -R "$source/." "$tmp/repo/"
rm -rf "$tmp/repo/.git"
cd "$tmp/repo"
git init -q
git checkout --orphan public-check >/dev/null 2>&1
git config user.name 'Public history check'
git config user.email 'public-check@example.com'
git config commit.gpgsign false
if [[ "$mode" = --positive-control ]]; then
  printf '%s.%s.%s.%s\n' 192 168 1 141 > history-positive-control.txt
fi
git add -f .
git commit -qm 'Initial public export' --allow-empty
if [[ "$mode" = --positive-control ]]; then
  git rm -q history-positive-control.txt
fi
git commit -qm 'Verify public history' --allow-empty
# Use the same guard as an actual first push; no remote refs can hide ancestors.
zero=0000000000000000000000000000000000000000
status=0
printf 'refs/heads/public-check %s refs/heads/public-check %s\n' "$(git rev-parse HEAD)" "$zero" | bash "$here/pre-push" origin || status=$?
echo "history-check: scanned $(git rev-list --count --all) reachable commits"
exit "$status"
