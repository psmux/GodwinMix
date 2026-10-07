# Sign the Windows builds

A `v*` tag builds the Windows installers and the core zip with an Authenticode
signature on every executable in them, so Windows and SmartScreen show the
publisher instead of "Unknown publisher". The certificate is a Certum code
signing certificate kept in Certum's cloud (SimplySign). Nobody has its
private key on disk, the release runner included.

This page is for whoever cuts releases. Nothing here is needed to build or run
GodwinMix.

## What is signed

On a `v*` tag, in `.github/workflows/release.yml`:

* The core job signs `godwinmix.exe` and `gmx.exe` before they go in the
  zip, so the zip and `SHA256SUMS` are made from the signed files.
* The desktop job signs every `.exe` and `.dll` the installers carry that is
  not signed already: the mixer sidecar, the plugins under `plugins/`
  (`gmx-camera.exe` and the rest), the browser renderer and its Chromium
  libraries, and the trimmed GStreamer runtime. Files that arrive signed by
  their makers keep that signature.
* Tauri then signs `godwinmix-desktop.exe` inside each installer, the NSIS
  uninstaller and plugins, the `-setup.exe` and the `.msi`, through
  `bundle.windows.signCommand`.
* Both installers are unpacked afterwards and every executable and library
  in them is checked. `godwinmix*` and `gmx*` files must carry this
  project's certificate. One unsigned file fails the job, and with it the
  release: a tag publishes signed Windows files or nothing.

Every signature has an RFC 3161 timestamp from `http://time.certum.pl`, so it
stays valid after the certificate expires.

A run started by hand from the Actions tab (a rehearsal) skips all of this and
builds unsigned files, because the signing secrets are only given to tag runs.
The macOS and Linux builds are not signed by this.

## How the runner logs in

SimplySign Desktop, once logged in, shows the cloud certificate to Windows as
a smart card, and `signtool` signs with it by thumbprint. Certum documents no
way to log in except its window, so `.github/windows-signing/login.ps1`:

1. downloads SimplySign Desktop's 64 bit `.msi` from Certum, refuses it unless
   its own signature is valid, and installs it silently;
2. computes the six digit code from the TOTP secret (RFC 6238, in
   `totp.ps1`, no module needed);
3. opens the login window and fills in the account and the code with
   `WM_SETTEXT`, then clicks Ok;
4. waits for a code signing certificate with a private key to appear in
   `Cert:\CurrentUser\My`, trying up to three codes.

`sign.ps1` signs in batches and retries a batch the timestamp server refused.
`verify.ps1` does the checking. `logout.ps1` stops SimplySign at the end of the
job.

## Set up the environment

In the repository's **Settings > Environments**, an environment named
`windows-signing` with:

* a deployment rule that allows tags matching `v*`, and nothing else;
* secret `CERTUM_USERNAME`: the e-mail address the SimplySign account logs
  in with;
* secret `CERTUM_TOTP_SECRET`: the Base32 secret behind the SimplySign
  mobile app's codes, the one shown as a QR code when the token was set up;
* variable `CERTUM_TOTP_ALGORITHM`: `SHA-256` for SimplySign. `SHA-1` and
  `SHA-512` are accepted too.

Only the Windows legs of the `core` and `desktop` jobs ask for the
environment. Every other leg, and every rehearsal, gets an empty environment
name, which GitHub treats as no environment.

## Test it without releasing

`.github/workflows/sign-test.yml` runs the login, signing and checks on a
small program, on any tag matching `v*-signtest*`. Such a tag is allowed into
the environment (it starts with `v`) and is ignored by `release.yml` and the
other tag workflows, so it publishes nothing.

```sh
git tag v0.2.2-signtest.1
git push origin v0.2.2-signtest.1
gh run watch "$(gh run list -w 'sign test' -L 1 --json databaseId --jq '.[0].databaseId')"
```

A good run prints the certificate it found, signs twenty copies (about thirty
seconds), and ends with `signtool verify /pa /v` showing the chain up to
Certum Trusted Network CA and "The signature is timestamped". Delete the tag
when it is done:

```sh
git push --delete origin v0.2.2-signtest.1
git tag -d v0.2.2-signtest.1
```

## When the login fails

The job log says which attempt failed and what SimplySign's window said, for
example "Invalid user name or token". The sign test also keeps pictures of
the login window for a day, as the `simplysign-screens` artifact, with the
account field emptied first.

* "Invalid user name or token" on all three tries: check the two secrets,
  and that the variable matches the token's algorithm.
* No login window: Certum may have moved the installer. Update
  `$SimplySignVersion` in `.github/windows-signing/simplysign.ps1` from
  [Certum's download page](https://support.certum.eu/en/cert-offer-software-and-libraries/).
* A batch that fails three times: usually the timestamp server. Run the
  release again.
