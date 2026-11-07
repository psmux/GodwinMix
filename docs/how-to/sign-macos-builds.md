# Sign the macOS builds

A `v*` tag builds the Mac app, its disk image and the core archive signed
with an Apple Developer ID and notarised by Apple. A Mac opens them on a
double click with no warning, offline too, since the ticket is stapled to the
app and to the disk image.

This page is for whoever cuts releases. Nothing here is needed to build or run
GodwinMix.

## What is signed

On a `v*` tag, in `.github/workflows/release.yml`, on both the Apple silicon
and the Intel legs:

* The core job signs `godwinmix` and `gmx` before they go in the archive, and
  sends them to Apple's notary service. A loose program cannot carry a stapled
  ticket, so macOS looks the ticket up the first time it runs.
* The desktop job lets Tauri build the app and its disk image as before, then
  signs every program in the app again with the Developer ID, inside out: the
  GStreamer libraries and plugins, the plugin binaries under `plugins/`, ONNX
  Runtime, the web page renderer with its Chromium framework and its five
  helper apps, the mixer, and last the app itself. 218 programs and bundles
  on Apple silicon in 0.3.31.
* The app is notarised and stapled. The disk image is made again from it with
  `hdiutil`, under the name Tauri gave it, then signed, notarised and stapled
  itself.
* The job mounts that disk image and asks Gatekeeper about the app on it.
  Anything but `source=Notarized Developer ID` fails the job, and with it the
  release. The app then runs `--headless-check`, under the hardened runtime
  it ships with.

Every program is signed with the hardened runtime and a secure timestamp,
which notarisation requires. `dev/sign-macos.sh` does all of it, and
`tauri-app/entitlements.plist` holds the exceptions to the hardened runtime
each program gets:

| Entitlement | Why |
|---|---|
| `cs.allow-jit`, `cs.allow-unsigned-executable-memory` | Chromium's V8 in the web page renderer, and wasmtime in a build with the `wasm` feature, write machine code as they run |
| `cs.disable-library-validation` | a GStreamer plugin or library somebody installs later is signed by somebody else, or not at all |
| `cs.allow-dyld-environment-variables` | the desktop app starts the mixer with `DYLD_LIBRARY_PATH` pointing at its own runtime |
| `device.camera`, `device.audio-input` | without them the hardened runtime denies the camera and the microphone, and macOS never asks |

A tag with any of the secrets below missing fails both macOS jobs in their
first minute. It does not fall back to an ad hoc build. A run started by hand
from the Actions tab signs and notarises too when the secrets are there, and
is signed ad hoc when they are not, as forks are.

## The secrets

Repository secrets, under Settings, Secrets and variables, Actions:

| Secret | What it holds |
|---|---|
| `APPLE_CERTIFICATE` | the Developer ID Application certificate and its private key, as a `.p12`, base64 encoded |
| `APPLE_CERTIFICATE_PASSWORD` | the `.p12` password |
| `APPLE_SIGNING_IDENTITY` | the identity's name as `security find-identity -v -p codesigning` prints it |
| `APPLE_TEAM_ID` | the ten character team id; every signature is checked against it |
| `APPLE_API_KEY` | an App Store Connect API key (`.p8`) with the Developer role, base64 encoded |
| `APPLE_API_KEY_ID` | that key's id |
| `APPLE_API_ISSUER` | the issuer id shown above the list of keys |

The certificate goes into a keychain made for the job, which is deleted at the
end of it, and the API key into a file that is deleted with it. The notary key
is tried before anything is built, so a wrong one fails in seconds.

## Signing on your own Mac

With a Developer ID in your login keychain, the same script signs and checks
an app you built or downloaded:

```sh
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID)"
dev/sign-macos.sh sign GodwinMix.app
dev/sign-macos.sh verify GodwinMix.app
spctl -a -vv -t exec GodwinMix.app
```

`spctl` says `source=Developer ID` until the app is notarised, and
`source=Notarized Developer ID` after. To notarise as well, set the three
`APPLE_API_*` variables and run `dev/sign-macos.sh notarize GodwinMix.app`.
When Apple refuses a submission, the script prints the notary log, which
names each file it objected to.

## See also

* [Install on macOS](install-on-macos.md)
* [Sign the Windows builds](sign-windows-builds.md)
* [Trust and signing](../explanation/trust-and-signing.md)
