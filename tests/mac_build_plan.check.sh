#!/bin/sh
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Vialian
# MAC-10, MAC-11: the macOS build script signs ad-hoc by default, notarizes only with a Developer ID identity and all
# three App Store Connect variables, refuses partial variables or missing files, and never prints a secret it reads.
# Run: sh tests/mac_build_plan.check.sh   (any POSIX shell; nothing is built)
set -eu
cd "$(dirname "$0")/.."
script=installers/macos/build.sh
work="$(mktemp -d)"
stub_target=symvoulio-check-target
trap 'rm -rf "$work" "target/$stub_target"' EXIT
printf 'P8-SECRET-CONTENT-123\n' > "$work/key.p8"
printf 'CERT-BASE64-SECRET-456\n' > "$work/cert.b64"
printf 'CERT-PASSWORD-SECRET-789\n' > "$work/cert.pw"
fail=0
# check NAME EXPECTED_EXIT EXPECTED_TEXT [VAR=value ...]
check() {
  name=$1
  want_exit=$2
  want=$3
  shift 3
  set +e
  out="$(env -i PATH="$PATH" HOME="$work" SYMVOULIO_BUILD_DRY_RUN=1 "$@" sh "$script" 2>&1)"
  code=$?
  set -e
  if [ "$code" != "$want_exit" ]; then
    echo "FAIL $name: exit $code, want $want_exit: $out"
    fail=1
  fi
  case "$out" in
    *"$want"*) ;;
    *) echo "FAIL $name: missing [$want] in: $out"; fail=1 ;;
  esac
  case "$out" in
    *SECRET*) echo "FAIL $name: a secret value was printed: $out"; fail=1 ;;
  esac
}
check "default is ad-hoc" 0 "target=universal-apple-darwin signing=adhoc notarize=no"
check "dash identity is ad-hoc" 0 "signing=adhoc notarize=no" APPLE_SIGNING_IDENTITY=-
check "API vars without Developer ID stay ad-hoc (D-039)" 0 "signing=adhoc notarize=no" \
  APPLE_API_ISSUER=issuer-id APPLE_API_KEY=KEYID APPLE_API_KEY_PATH="$work/key.p8"
check "Developer ID without API vars signs, no notarization" 0 "signing=developer-id notarize=no" \
  "APPLE_SIGNING_IDENTITY=Developer ID Application: Example (TEAMID)"
check "Developer ID and API vars notarize the dmg" 0 "notarytool submit --wait, stapler staple" \
  "APPLE_SIGNING_IDENTITY=Developer ID Application: Example (TEAMID)" APPLE_API_ISSUER=issuer-id APPLE_API_KEY=KEYID \
  APPLE_API_KEY_PATH="$work/key.p8" APPLE_CERTIFICATE_PATH="$work/cert.b64" APPLE_CERTIFICATE_PASSWORD_PATH="$work/cert.pw"
check "partial API vars are refused" 2 "needs all three" APPLE_API_ISSUER=issuer-id APPLE_API_KEY=KEYID
check "missing key file is refused" 2 "APPLE_API_KEY_PATH does not name a readable file" \
  APPLE_API_ISSUER=issuer-id APPLE_API_KEY=KEYID APPLE_API_KEY_PATH="$work/absent.p8"
check "missing certificate file is refused" 2 "APPLE_CERTIFICATE_PATH does not name a readable file" \
  APPLE_CERTIFICATE_PATH="$work/absent.b64"
check "target override" 0 "target=aarch64-apple-darwin" SYMVOULIO_MAC_TARGET=aarch64-apple-darwin
check "certificate without its password file is refused" 2 "APPLE_CERTIFICATE_PATH needs APPLE_CERTIFICATE_PASSWORD_PATH" \
  "APPLE_SIGNING_IDENTITY=Developer ID Application: Example (TEAMID)" APPLE_CERTIFICATE_PATH="$work/cert.b64"

# ---- Real execution with stand-in tools (review finding: dry runs never read the secrets) ----
# Every tool the script calls is a stub that logs its arguments and whether APPLE_CERTIFICATE / APPLE_API_KEY reach
# it; npx creates the bundle folders the real `tauri build` would.
stubs="$work/bin"
mkdir -p "$stubs"
for tool in codesign xcrun security openssl shasum; do
  cat > "$stubs/$tool" <<'STUB'
#!/bin/sh
printf '%s %s\n' "$(basename "$0")" "$*" >> "$STUB_LOG"
case "$(basename "$0") $1" in
  "openssl rand") echo generated-keychain-password ;;
  "security list-keychains") [ "$4" = "-s" ] || {
      printf '    "/Users/someone/Library/Keychains/login.keychain-db"\n'
      # A hostile keychain path: it must stay data (review round 2), never run.
      printf '    "/Users/someone/$(touch %s)/x`touch %s`.keychain-db"\n' "$INJECT_MARK" "$INJECT_MARK"
    } ;;
  "shasum -a") echo "0000  $3" ;;
esac
exit 0
STUB
done
cat > "$stubs/base64" <<'STUB'
#!/bin/sh
printf 'base64 %s\n' "$*" >> "$STUB_LOG"
cat
STUB
cat > "$stubs/npx" <<'STUB'
#!/bin/sh
printf 'npx %s cert_env=%s api_key_env=%s\n' "$*" "${APPLE_CERTIFICATE:+set}" "${APPLE_API_KEY:+set}" >> "$STUB_LOG"
mkdir -p "target/$SYMVOULIO_MAC_TARGET/release/bundle/dmg" "target/$SYMVOULIO_MAC_TARGET/release/bundle/macos/Symvoulio.app"
: > "target/$SYMVOULIO_MAC_TARGET/release/bundle/dmg/Symvoulio_0.3.2_universal.dmg"
STUB
chmod +x "$stubs"/*
# run NAME EXPECTED_LOG_SUBSTRINGS... -- VAR=value...
run() {
  name=$1
  shift
  log="$work/$name.log"
  : > "$log"
  wants=""
  while [ "$1" != "--" ]; do wants="$wants$1
"; shift; done
  shift
  set +e
  out="$(env -i PATH="$stubs:$PATH" HOME="$work" STUB_LOG="$log" INJECT_MARK="$work/injected" SYMVOULIO_MAC_TARGET="$stub_target" "$@" sh "$script" 2>&1)"
  code=$?
  set -e
  [ "$code" = 0 ] || { echo "FAIL run $name: exit $code: $out"; fail=1; }
  case "$out" in *SECRET*) echo "FAIL run $name: a secret value was printed: $out"; fail=1 ;; esac
  printf '%s' "$wants" | while IFS= read -r want; do
    [ -z "$want" ] || grep -qF -- "$want" "$log" || { echo "FAIL run $name: log lacks [$want]"; cat "$log"; exit 1; }
  done || fail=1
}
run adhoc "npx tauri build --target $stub_target --bundles app,dmg cert_env= api_key_env=" "codesign --verify --deep --strict" "shasum -a 256" -- \
  APPLE_API_ISSUER=issuer-id APPLE_API_KEY=KEYID APPLE_API_KEY_PATH="$work/key.p8"
if grep -q -e "^security" -e "--sign" "$work/adhoc.log"; then echo "FAIL run adhoc: signed or touched a keychain"; fail=1; fi
run developer-id "security create-keychain" "security import" "cert_env= api_key_env=set" "codesign --force --sign" \
  "xcrun notarytool submit" "xcrun stapler staple" "security delete-keychain" -- \
  "APPLE_SIGNING_IDENTITY=Developer ID Application: Example (TEAMID)" APPLE_API_ISSUER=issuer-id APPLE_API_KEY=KEYID \
  APPLE_API_KEY_PATH="$work/key.p8" APPLE_CERTIFICATE_PATH="$work/cert.b64" APPLE_CERTIFICATE_PASSWORD_PATH="$work/cert.pw"
# Order: the keychain exists before tauri build and is deleted only after the .dmg is stapled.
log="$work/developer-id.log"
line() { grep -n -F -- "$1" "$log" | head -n 1 | cut -d: -f1; }
create=$(line "security create-keychain"); build=$(line "npx tauri build"); staple=$(line "xcrun stapler staple"); delete=$(line "security delete-keychain")
if ! { [ -n "$create" ] && [ -n "$delete" ] && [ "$create" -lt "$build" ] && [ "$build" -lt "$staple" ] && [ "$staple" -lt "$delete" ]; }; then
  echo "FAIL run developer-id: keychain lifetime create=$create build=$build staple=$staple delete=$delete"
  fail=1
fi
# The certificate content reaches only base64 via stdin, never any argument list.
if grep -q "CERT-BASE64-SECRET" "$log"; then echo "FAIL run developer-id: certificate content in an argument list"; fail=1; fi
# The search list gets our keychain first, then exactly the saved paths, and is restored at exit; the hostile path is
# passed as one literal argument and nothing in it runs.
hostile='/Users/someone/$(touch '"$work/injected"')/x`touch '"$work/injected"'`.keychain-db'
added=$(grep -F -- "security list-keychains -d user -s " "$log" | grep -F "symvoulio-signing.keychain-db" | head -n 1)
case "$added" in
  *"symvoulio-signing.keychain-db /Users/someone/Library/Keychains/login.keychain-db $hostile") ;;
  *) echo "FAIL run developer-id: search list with our keychain is [$added]"; fail=1 ;;
esac
restored=$(grep -F -- "security list-keychains -d user -s " "$log" | grep -vF "symvoulio-signing" | tail -n 1)
[ "$restored" = "security list-keychains -d user -s /Users/someone/Library/Keychains/login.keychain-db $hostile" ] || {
  echo "FAIL run developer-id: search list not restored: [$restored]"; fail=1; }
if [ -e "$work/injected" ]; then echo "FAIL run developer-id: a keychain path was executed"; fail=1; fi
# The macOS config signs ad-hoc and targets macOS 13; the Windows targets stay in the shared config (MAC-02).
grep -q '"signingIdentity": "-"' src-tauri/tauri.macos.conf.json || { echo "FAIL ad-hoc identity missing"; fail=1; }
grep -q '"minimumSystemVersion": "13.0"' src-tauri/tauri.macos.conf.json || { echo "FAIL macOS 13 minimum missing"; fail=1; }
tr -d ' \r\n' < src-tauri/tauri.conf.json | grep -q '"targets":\["msi","nsis"\]' || { echo "FAIL Windows targets changed"; fail=1; }
if grep -n 'set -x' "$script"; then
  echo "FAIL xtrace would echo secrets"
  fail=1
fi
[ "$fail" = 0 ] && echo "mac build plan check passed"
exit "$fail"
