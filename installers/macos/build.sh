#!/bin/sh
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Vialian
#
# Build Symvoulio for macOS on a Mac: one universal Symvoulio.app (Apple Silicon and Intel) and a .dmg with it and an
# Applications shortcut (MAC-01, VAL.3, VAL.4), plus the .dmg's SHA-256 next to it.
#
# Signing (VAL.6, MAC-10), from Tauri's "macOS Code Signing" page (https://v2.tauri.app/distribute/sign/macos/,
# last updated 2026-05-17, read 2026-11-05):
#   - default: ad-hoc signature ("signingIdentity": "-" in src-tauri/tauri.macos.conf.json); no notarization;
#   - APPLE_SIGNING_IDENTITY set to a "Developer ID Application" identity: signed with it;
#   - additionally APPLE_API_ISSUER, APPLE_API_KEY (the Key ID) and APPLE_API_KEY_PATH (the .p8 file): Tauri
#     notarizes the app, and this script signs, notarizes and staples the .dmg itself.
# Secrets (MAC-11) come only from files named by environment variables, never from the command line:
#   APPLE_API_KEY_PATH, and for a Mac without the certificate in its keychain APPLE_CERTIFICATE_PATH (base64 .p12)
#   with APPLE_CERTIFICATE_PASSWORD_PATH. The certificate goes into a temporary keychain that lives until the .dmg
#   is signed and notarized, and is deleted on exit (Tauri's own APPLE_CERTIFICATE import would delete its keychain
#   when `tauri build` ends, before the .dmg is signed). The script never prints a value read from those files and
#   never runs with xtrace.
#
# SYMVOULIO_BUILD_DRY_RUN=1 prints the plan and exits without building (tests/mac_build_plan.check.sh).
# SYMVOULIO_MAC_TARGET overrides the Rust target (default universal-apple-darwin).
set -eu
cd "$(dirname "$0")/../.."

fail() {
  echo "error: $1" >&2
  exit 2
}
target="${SYMVOULIO_MAC_TARGET:-universal-apple-darwin}"
mode=adhoc
notarize=no
if [ -n "${APPLE_SIGNING_IDENTITY:-}" ] && [ "${APPLE_SIGNING_IDENTITY}" != "-" ]; then
  mode=developer-id
fi
api_vars=0
[ -z "${APPLE_API_ISSUER:-}" ] || api_vars=$((api_vars + 1))
[ -z "${APPLE_API_KEY:-}" ] || api_vars=$((api_vars + 1))
[ -z "${APPLE_API_KEY_PATH:-}" ] || api_vars=$((api_vars + 1))
if [ "$api_vars" -gt 0 ] && [ "$api_vars" -lt 3 ]; then
  fail "notarization needs all three of APPLE_API_ISSUER, APPLE_API_KEY, APPLE_API_KEY_PATH"
fi
if [ "$api_vars" -eq 3 ]; then
  [ -f "$APPLE_API_KEY_PATH" ] || fail "APPLE_API_KEY_PATH does not name a readable file"
  if [ "$mode" = developer-id ]; then
    notarize=yes
  else
    # D-039: no Apple Developer Program yet. Apple notarizes only Developer ID signatures.
    echo "warning: APPLE_API_* are set but APPLE_SIGNING_IDENTITY is not a Developer ID identity; building ad-hoc, not notarized" >&2
  fi
fi
if [ -n "${APPLE_CERTIFICATE_PATH:-}" ]; then
  [ -f "$APPLE_CERTIFICATE_PATH" ] || fail "APPLE_CERTIFICATE_PATH does not name a readable file"
  [ -n "${APPLE_CERTIFICATE_PASSWORD_PATH:-}" ] || fail "APPLE_CERTIFICATE_PATH needs APPLE_CERTIFICATE_PASSWORD_PATH"
fi
if [ -n "${APPLE_CERTIFICATE_PASSWORD_PATH:-}" ] && [ ! -f "$APPLE_CERTIFICATE_PASSWORD_PATH" ]; then
  fail "APPLE_CERTIFICATE_PASSWORD_PATH does not name a readable file"
fi

echo "Symvoulio macOS build: target=$target signing=$mode notarize=$notarize"
if [ "${SYMVOULIO_BUILD_DRY_RUN:-}" = 1 ]; then
  [ "$mode" = adhoc ] || [ -z "${APPLE_CERTIFICATE_PATH:-}" ] || echo "plan: temporary signing keychain with the certificate"
  echo "plan: npx tauri build --target $target --bundles app,dmg"
  [ "$notarize" = no ] || echo "plan: codesign, notarytool submit --wait, stapler staple <dmg>"
  exit 0
fi

# Tauri must never import a certificate itself (its keychain would be gone before the .dmg is signed).
unset APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD
keychain=""
keychain_dir=""
saved_list=""
# Set the user keychain search list to [$1] + the saved list. Paths are data: read line by line into arguments,
# never evaluated, so a path with $(...) or backticks cannot run anything.
search_list() {
  extra="${1:-}"
  set --
  [ -z "$extra" ] || set -- "$extra"
  while IFS= read -r saved; do
    [ -z "$saved" ] || set -- "$@" "$saved"
  done < "$saved_list"
  security list-keychains -d user -s "$@"
}
cleanup() {
  if [ -n "$keychain" ]; then
    [ ! -f "$saved_list" ] || search_list >/dev/null 2>&1 || true
    security delete-keychain "$keychain" >/dev/null 2>&1 || true
  fi
  [ -z "$keychain_dir" ] || rm -rf "$keychain_dir"
}
trap cleanup EXIT
trap 'exit 130' INT TERM

if [ "$mode" = adhoc ]; then
  # Never let a stray API key make Tauri try to notarize an ad-hoc build.
  unset APPLE_SIGNING_IDENTITY APPLE_API_ISSUER APPLE_API_KEY APPLE_API_KEY_PATH
elif [ -n "${APPLE_CERTIFICATE_PATH:-}" ]; then
  keychain_dir="$(mktemp -d)"
  keychain="$keychain_dir/symvoulio-signing.keychain-db"
  keychain_password="$(openssl rand -hex 24)"
  security create-keychain -p "$keychain_password" "$keychain" >/dev/null
  security set-keychain-settings -lut 21600 "$keychain"
  security unlock-keychain -p "$keychain_password" "$keychain" >/dev/null
  base64 -d < "$APPLE_CERTIFICATE_PATH" > "$keychain_dir/certificate.p12"
  security import "$keychain_dir/certificate.p12" -k "$keychain" -P "$(cat "$APPLE_CERTIFICATE_PASSWORD_PATH")" \
    -T /usr/bin/codesign >/dev/null
  rm -f "$keychain_dir/certificate.p12"
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$keychain_password" "$keychain" >/dev/null
  # `security list-keychains` prints one quoted path per line; keep the paths without the quotes.
  saved_list="$keychain_dir/search-list.txt"
  security list-keychains -d user | sed -e 's/^[[:space:]]*"//' -e 's/"[[:space:]]*$//' > "$saved_list"
  search_list "$keychain"
fi

npx tauri build --target "$target" --bundles app,dmg

bundle="target/$target/release/bundle"
dmg="$(ls -t "$bundle"/dmg/*.dmg | head -n 1)"
if [ "$notarize" = yes ]; then
  codesign --force --sign "$APPLE_SIGNING_IDENTITY" --timestamp "$dmg"
  xcrun notarytool submit "$dmg" --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER" --wait
  xcrun stapler staple "$dmg"
fi
codesign --verify --deep --strict "$bundle/macos/Symvoulio.app"
(cd "$bundle/dmg" && shasum -a 256 "$(basename "$dmg")" > "$(basename "$dmg").sha256")
echo "dmg: $dmg"
cat "$dmg.sha256"
