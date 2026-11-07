#!/usr/bin/env bash
# Developer ID signing and notarisation for the macOS release.
#
#   dev/sign-macos.sh check                    the seven secrets are all set
#   dev/sign-macos.sh keychain                 import APPLE_CERTIFICATE into a throwaway keychain
#   dev/sign-macos.sh preflight                Apple accepts the notarisation login
#   dev/sign-macos.sh sign <GodwinMix.app>     every program inside out, then the app
#   dev/sign-macos.sh sign <file>...           loose programs, such as the core archive's
#   dev/sign-macos.sh verify <path>...         Developer ID, hardened runtime and a timestamp on every program
#   dev/sign-macos.sh dmg <app> <out.dmg>      a disk image of the signed app, signed itself
#   dev/sign-macos.sh notarize <path>          submit, wait, and staple an app or a disk image
#   dev/sign-macos.sh cleanup                  delete the keychain and the API key file again
#
# Reads APPLE_SIGNING_IDENTITY, and for notarisation the App Store Connect API
# key: APPLE_API_KEY (a base64 .p8), APPLE_API_KEY_ID and APPLE_API_ISSUER.
# `keychain` also reads APPLE_CERTIFICATE (a base64 .p12) and
# APPLE_CERTIFICATE_PASSWORD. On a Mac whose login keychain has the identity,
# `sign` and `verify` need only APPLE_SIGNING_IDENTITY.
# docs/how-to/sign-macos-builds.md has the story.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
entitlements="$here/../tauri-app/entitlements.plist"
secrets=(APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_SIGNING_IDENTITY APPLE_TEAM_ID APPLE_API_KEY APPLE_API_KEY_ID APPLE_API_ISSUER)

die() { echo "::error::$*" >&2; exit 1; }

check() {
  local missing=()
  for name in "${secrets[@]}"; do [ -n "${!name:-}" ] || missing+=("$name"); done
  [ ${#missing[@]} -eq 0 ] && { echo "all seven Apple signing secrets are set"; return; }
  die "a release tag ships a Developer ID signed and notarised Mac app or nothing, and these repository secrets are empty: ${missing[*]}. Add them under Settings, Secrets and variables, Actions (docs/how-to/sign-macos-builds.md), then run the tag again."
}

keychain() {
  local kc="$keychain_file" pw p12="$RUNNER_TEMP/godwinmix-cert.p12"
  pw=$(uuidgen)
  security create-keychain -p "$pw" "$kc"
  # Without this the keychain locks again mid job and codesign fails with a
  # bare "User interaction is not allowed".
  security set-keychain-settings -lut 21600 "$kc"
  security unlock-keychain -p "$pw" "$kc"
  printf '%s' "$APPLE_CERTIFICATE" | base64 --decode > "$p12"
  security import "$p12" -k "$kc" -P "$APPLE_CERTIFICATE_PASSWORD" -T /usr/bin/codesign -f pkcs12
  rm -f "$p12"
  security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$pw" "$kc" >/dev/null
  # Prepended, so the login keychain stays on the search list.
  local list=()
  while IFS= read -r k; do list+=("$k"); done < <(security list-keychains -d user | sed 's/^[[:space:]]*"//; s/"[[:space:]]*$//')
  security list-keychains -d user -s "$kc" "${list[@]}"
  [[ "$(security find-identity -v -p codesigning "$kc")" == *"Developer ID Application"* ]] \
    || die "APPLE_CERTIFICATE holds no Developer ID Application identity"
}

notary() {
  if [ ! -s "$key_file" ]; then
    (umask 077; printf '%s' "$APPLE_API_KEY" | base64 --decode > "$key_file")
  fi
  xcrun notarytool "$@" --key "$key_file" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER"
}

# notarytool turns a wrong key away in seconds, where finding out at the end
# of the build costs the whole hour.
preflight() {
  notary history >/dev/null 2>"$RUNNER_TEMP/notary.err" && { echo "Apple accepted the notarisation key"; return; }
  cat "$RUNNER_TEMP/notary.err" >&2 || true
  die "Apple refused the notarisation key. APPLE_API_KEY must be the base64 of an App Store Connect API key (.p8) with the Developer role, APPLE_API_KEY_ID its key id and APPLE_API_ISSUER the issuer id on the same page."
}

cleanup() {
  rm -f "$key_file"
  [ -e "$keychain_file" ] && security delete-keychain "$keychain_file" || true
}

# Matched in the shell, not piped to grep -q: under pipefail grep leaving
# early makes `file` die of SIGPIPE, and the program was skipped unsigned.
is_macho() { [[ "$(file -b "$1")" == Mach-O* ]]; }
is_executable() { [[ "$(file -b "$1")" == *executable* ]]; }

sign_one() {
  local args=(--force --sign "$APPLE_SIGNING_IDENTITY" --options runtime --timestamp)
  case "$1" in
    *.app) args+=(--entitlements "$entitlements") ;;
    *.framework) ;;
    *) is_executable "$1" && args+=(--entitlements "$entitlements") ;;
  esac
  codesign "${args[@]}" "$1"
}

# Inside out: deepest path first, so every file is signed before the bundle
# whose seal covers it, and the app itself last. A bundle's main executable is
# signed once on its own and again as part of its bundle, which is harmless.
sign_app() {
  local app="${1%/}" n=0
  # On descriptor 3, so nothing the loop runs can read the list from stdin.
  while IFS= read -r -d '' item <&3; do
    sign_one "$item" </dev/null; n=$((n + 1))
  done 3< <(
    { find "$app/Contents" -type f -print0 | while IFS= read -r -d '' f; do if is_macho "$f"; then printf '%s\0' "$f"; fi; done
      find "$app/Contents" -type d \( -name '*.app' -o -name '*.framework' \) -print0; } |
      perl -0ne 'chomp; push @a, $_; END { print "$_\0" for sort { ($b =~ tr{/}{}) <=> ($a =~ tr{/}{}) or $a cmp $b } @a }'
  )
  sign_one "$app"
  echo "signed $n programs and bundles inside $app, then $app"
}

sign() {
  [ -n "${APPLE_SIGNING_IDENTITY:-}" ] || die "APPLE_SIGNING_IDENTITY is empty"
  for path in "$@"; do
    case "${path%/}" in *.app) sign_app "$path" ;; *) sign_one "$path" ;; esac
  done
}

verify_one() {
  local info
  info=$(codesign -dvv "$1" 2>&1) || die "$1 is not signed"
  grep -q '^Authority=Developer ID Application' <<<"$info" || die "$1 is not signed with a Developer ID"
  grep -q 'flags=.*runtime' <<<"$info" || die "$1 is signed without the hardened runtime, which notarisation refuses"
  grep -q '^Timestamp=' <<<"$info" || die "$1 has no secure timestamp, which notarisation refuses"
  if [ -n "${APPLE_TEAM_ID:-}" ]; then
    grep -q "^TeamIdentifier=$APPLE_TEAM_ID\$" <<<"$info" || die "$1 is signed by another team than APPLE_TEAM_ID"
  fi
}

verify() {
  for path in "$@"; do
    if [ -d "$path" ]; then
      codesign --verify --deep --strict "$path" || die "$path does not verify"
      local n=0
      while IFS= read -r -d '' f; do
        is_macho "$f" || continue
        verify_one "$f"; n=$((n + 1))
      done < <(find "$path" -type f -print0)
      verify_one "$path"
      echo "$path and the $n programs in it carry a Developer ID signature with the hardened runtime"
    else
      codesign --verify --strict "$path" || die "$path does not verify"
      verify_one "$path"
      echo "$path carries a Developer ID signature with the hardened runtime"
    fi
  done
}

# hdiutil rather than Tauri's disk image, which wraps the app Tauri signed
# ad hoc. On a CI runner Tauri skips the Finder window layout anyway, so the
# two images hold the same thing: the app and a link to Applications.
dmg() {
  local app="${1%/}" out="$2" stage
  stage=$(mktemp -d)
  ditto "$app" "$stage/$(basename "$app")"
  ln -s /Applications "$stage/Applications"
  rm -f "$out"
  hdiutil create -volname "$(basename "$app" .app)" -srcfolder "$stage" -fs HFS+ -format UDZO -ov "$out" >/dev/null
  rm -rf "$stage"
  codesign --force --sign "$APPLE_SIGNING_IDENTITY" --timestamp "$out"
  echo "made and signed $out"
}

# An app goes up as a zip, since notarytool takes no directories, and the
# ticket is stapled to the app itself so it opens with no network. A loose
# program cannot carry a ticket, and macOS asks Apple for it on first launch.
notarize() {
  local path="${1%/}" upload id status
  case "$path" in
    *.app) upload="$RUNNER_TEMP/$(basename "$path").zip"; ditto -c -k --sequesterRsrc --keepParent "$path" "$upload" ;;
    *.dmg|*.zip) upload="$path" ;;
    *) die "notarize takes an .app, a .dmg or a .zip, not $path" ;;
  esac
  notary submit "$upload" --wait --timeout 45m --output-format json > "$RUNNER_TEMP/notary.json"
  id=$(plutil -extract id raw "$RUNNER_TEMP/notary.json")
  status=$(plutil -extract status raw "$RUNNER_TEMP/notary.json")
  if [ "$status" != Accepted ]; then
    notary log "$id" >&2 || true
    die "Apple's notary service answered $status for $path (submission $id); its log is above"
  fi
  [ "$upload" = "$path" ] || rm -f "$upload"
  case "$path" in *.zip) echo "notarised $path"; return ;; esac
  # stapler fetches the ticket from Apple's CloudKit, and on the Intel runner
  # the connection dropped twice running, after Apple had accepted the app.
  # The notarisation stands without the staple: Gatekeeper looks the ticket up
  # online at first launch, and the release checks for exactly that verdict.
  local try
  for try in 1 2 3 4 5 6; do
    if xcrun stapler staple "$path" && xcrun stapler validate "$path"; then
      echo "notarised and stapled $path"; return
    fi
    [ "$try" -lt 6 ] && { echo "stapling $path failed, trying again in $((try * 30)) s"; sleep $((try * 30)); }
  done
  echo "::warning::$path is notarised, but Apple's ticket server would not hand over the ticket to staple; macOS fetches it online on first launch instead"
  return 0
}

RUNNER_TEMP="${RUNNER_TEMP:-$(mktemp -d)}"
keychain_file="$RUNNER_TEMP/godwinmix-signing.keychain-db"
key_file="$RUNNER_TEMP/godwinmix-notary.p8"
cmd="${1:-}"; shift || true
case "$cmd" in
  check|keychain|preflight|cleanup) "$cmd" ;;
  sign|verify) [ $# -gt 0 ] || die "$cmd needs a path"; "$cmd" "$@" ;;
  dmg) [ $# -eq 2 ] || die "dmg needs the app and the disk image to write"; dmg "$@" ;;
  notarize) [ $# -eq 1 ] || die "notarize needs one path"; notarize "$1" ;;
  *) sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
