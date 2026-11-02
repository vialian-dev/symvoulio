#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Vialian
# Fail if any file under <dir> matches a pattern from forbidden.txt (case-insensitive) or is a forbidden path.
# usage: tools/public-export/check.sh <dir>
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd); dir=${1:?usage: check.sh <dir>}; dir=${dir%/}
pat=$(grep -vE '^\s*(#|$)' "$here/forbidden.txt" | paste -sd'|')
bad=0
# The owner-approved product phrase is public only in the top-level README.md and RELEASE_NOTES.md, and only verbatim;
# any other form or place of the forbidden name still fails. Files are scanned one by one so the allowed path is
# compared as a whole path, never parsed back out of grep's colon-separated output.
allow='M''ensurabilitas Automatic Coding Pipeline (M''ACP)'  # split so this file passes its own check
acro='M''ACP'
hits=''; macp=''
while IFS= read -r -d '' f; do
  rel=${f#"$dir"/}
  lines=$(grep -IniE -e "$pat" -- "$f" || true)
  if [ -n "$lines" ]; then
    if [ "$rel" = README.md ] || [ "$rel" = RELEASE_NOTES.md ]; then
      lines=$(printf '%s
' "$lines" | while IFS= read -r l; do
        rest=${l#*:}; rest=${rest//"$allow"/}
        printf '%s
' "$rest" | grep -qiE -e "$pat" && printf '%s
' "$l"
      done)
    fi
    [ -n "$lines" ] && hits+=$(printf '%s
' "$lines" | sed "s#^#$f:#")$'
'
  fi
  # The acronym is public only inside the exact phrase.
  lines=$(grep -In -e "$acro" -- "$f" || true)
  if [ -n "$lines" ]; then
    lines=$(printf '%s
' "$lines" | while IFS= read -r l; do
      rest=${l#*:}; rest=${rest//"$allow"/}
      printf '%s
' "$rest" | grep -qE -e "(^|[^[:alnum:]_])$acro([^[:alnum:]_]|\$)" && printf '%s
' "$l"
    done)
    [ -n "$lines" ] && macp+=$(printf '%s
' "$lines" | sed "s#^#$f:#")$'
'
  fi
done < <(find "$dir" -type d \( -name node_modules -o -name .git \) -prune -o -type f -print0)
if [ -n "$hits" ]; then echo "FORBIDDEN STRINGS:"; printf '%s' "$hits" | head -200; bad=1; fi
if [ -n "$macp" ]; then echo "UNEXPANDED M""ACP (use the full phrase):"; printf '%s' "$macp" | head -200; bad=1; fi
for p in design telemetry docs SPEC_"owner.md" LINKEDIN_"POSTS_owner.md"; do
  [ -e "$dir/$p" ] && { echo "FORBIDDEN PATH: $p"; bad=1; }
done
ph=$(grep -rIlE "PLACE"'HOLDER' "$dir" --exclude-dir=node_modules --exclude-dir=.git | sed "s#^$dir/##" | paste -sd" " || true)
[ -n "$ph" ] && echo "placeholders to replace before publishing: $ph"
[ $bad -eq 0 ] && echo "public check: clean ($(find "$dir" -type f | wc -l) files)" || { echo "public check: FAILED"; exit 1; }
