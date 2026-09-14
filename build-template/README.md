# godwinmix-build-template

A custom build of GodwinMix is a preset plus your branding, under your own
name. This repository is the CI half of it. You fork it, set four values in
`build.toml`, push a tag, and a workflow hands you installers for Linux, macOS
and Windows with your product name on them.

Nothing here is a fork of GodwinMix. The workflow checks the mixer out at a tag
you pin and builds it, so upgrading to a new core is one line in `build.toml`.

## What you get

For every tag you push, attached to that tag's GitHub release:

| File | Platform |
|---|---|
| `<product>_<version>_amd64.deb` | Linux |
| `<product>_<version>_x64_en-US.msi` | Windows |
| `<product>_<version>_aarch64.dmg` and `macos-app.tar.gz` | macOS |
| `SHA256SUMS` | all of them |

Inside each one: the GodwinMix core, the preset you chose, your window title,
your icon, and a codec catalogue with every copyleft entry removed. The last
one is not optional and not configurable. `gmx build` refuses to bundle a GPL
codec, which is why openh264 and SVT-AV1 are always present as the fallbacks a
closed build encodes with.

## Do this once

1. Press "Use this template" on GitHub, or fork this repository.
2. Open `build.toml`. It is the only file you edit. Set the preset your product
   is made of, the product name, the identifier, and the path to your icon.
3. Put a square PNG, 512x512 or larger, in `branding/` and point the `icon`
   line at it. It ships empty, which means the stock GodwinMix icon, so the
   first build works before you have drawn anything.
4. Commit and push.

That is the whole of the setup. You have not written any CI and you have not
copied any GodwinMix source.

## Every release

```sh
git tag v1.0.0
git push origin v1.0.0
```

The tag name becomes the version on the installers, so use `vMAJOR.MINOR.PATCH`.
Watch the run under Actions. It builds the three platforms in parallel and then
creates the release. You can also start it by hand from the Actions tab
(`workflow_dispatch`), which builds the installers as artifacts and creates no
release, which is what to do the first time.

## What it costs

Three runners, from a cold cache, building the mixer and the desktop app:

| Runner | Wall clock | GitHub's multiplier | Billed minutes |
|---|---|---|---|
| ubuntu-latest | 25 to 40 minutes | 1x | 25 to 40 |
| windows-latest | 30 to 50 minutes | 2x | 60 to 100 |
| macos-latest | 25 to 40 minutes | 10x | 250 to 400 |

So a release costs somewhere around 350 to 550 billed minutes. On a public
repository that is free. On a private one the free allowance is 2,000 minutes a
month, which is three or four releases, and after that you are paying. The
cargo cache takes 30 to 50 percent off the second run onwards as long as the
pinned GodwinMix tag has not moved.

If macOS is what makes this expensive and you do not need it, delete the
`macos-latest` row from the matrix in `.github/workflows/build.yml`. The other
two keep working.

## When a new GodwinMix core comes out

Raise `tag` under `[upstream]` in `build.toml`, push a new tag of your own, and
collect the new installers. That is the entire upgrade. There is no fork to
rebase, no patch to reapply, and no merge conflict, because your product is a
preset and a `tauri.conf.json` fragment rather than modified source.

If you changed the preset on a working machine in the meantime, `gmx preset
save <name>` on that machine writes the new preset out, and you point
`build.toml` at it.

## Signing, and what needs an Apple developer account

Read this before you promise anybody a download link.

**Linux (.deb): nothing.** No account, no certificate, no fee. The workflow
produces an installable package as it stands.

**Windows (.msi): nothing required, but expect a warning.** An unsigned
installer builds and runs. SmartScreen shows "Windows protected your PC" and
the user has to click through "More info" and then "Run anyway". Removing that
means buying a code signing certificate from a commercial certificate authority
(roughly $200 to $400 a year, and an EV certificate costs more and clears the
SmartScreen reputation check sooner). This has nothing to do with Apple.

**macOS (.app and .dmg): an Apple Developer Program membership, $99 a year.**
Without it the build still succeeds and the result still runs, but every person
who downloads it gets "cannot be opened because the developer cannot be
verified" and has to right click, choose Open, and confirm. With it, add these
repository secrets and the workflow signs and notarises on its own:

| Secret | What it is |
|---|---|
| `APPLE_CERTIFICATE` | your Developer ID Application certificate, exported as a .p12 and base64 encoded |
| `APPLE_CERTIFICATE_PASSWORD` | the password you put on that export |
| `APPLE_SIGNING_IDENTITY` | the certificate's name, for example `Developer ID Application: Acme Ltd (TEAMID)` |
| `APPLE_ID` | the Apple ID that owns the membership |
| `APPLE_PASSWORD` | an app specific password for that Apple ID, not the account password |
| `APPLE_TEAM_ID` | your ten character team id |

Set none of them and nothing breaks; the macOS build comes out unsigned. Set
all of them and it comes out notarised. There is no middle state worth having:
a signed but un-notarised app is refused on current macOS anyway.

## Plugins and notarisation

A GodwinMix plugin is a separate process. The core spawns it as a child, talks
to it over a pipe with JSON-RPC, and reaps it on shutdown. It is never loaded
into the mixer's address space.

That matters here because the rule people have in mind when they worry about
this is the one for in process plugins: a hardened runtime refuses to load a
library that your certificate did not sign, so a host application that loads
third party plugins has to either disable library validation or get every
plugin signed. None of that applies. Your notarised build runs a plugin written
by somebody else, unsigned, without an entitlement and without a change to your
bundle, because from the operating system's point of view it is a program
starting another program.

The plugin's own binary still has to be something macOS will execute, which for
a downloaded binary means the person who ships the plugin deals with Gatekeeper
for their own file. That is their problem, and it does not reach into yours.

## What `gmx build` writes

The workflow runs one command on each runner:

```sh
gmx build --preset church --name "AcmeMix" --identifier com.acme.mix \
          --icon branding/icon.png --out dist
```

and it writes:

| Path | What it is |
|---|---|
| `dist/bin/` | the core binary this build ships |
| `dist/preset/` | the preset, whole. `gmx preset apply ./preset` applies it |
| `dist/godwinmix.toml` | the configuration the installer drops beside the binary |
| `dist/codecs.toml` | the codec catalogue, permissive entries only |
| `dist/theme.css` | the theme, when the preset ships one |
| `dist/tauri.conf.json` | the branding, merged over the upstream desktop config |
| `dist/icons/icon.png` | the icon, when one was given |
| `dist/README.md` | what CI does with all of this |

The flags are `--preset`, `--name`, `--icon`, `--out`, `--identifier` and
`--no-binary`, and `gmx build --help` is the authority on them.

You can run the same command on your own machine against a checkout of
GodwinMix, look at what comes out, and only then push a tag. That is the
fastest way to find out that you pointed `preset` at something that does not
exist.

## How the branding reaches the installer

`gmx build` writes a fragment rather than a whole `tauri.conf.json`, because a
copy of the full file would stop tracking upstream the moment the desktop app
gained a setting. The workflow merges the fragment over
`godwinmix/tauri-app/tauri.conf.json` field by field: product name, identifier,
publisher, version, and the title of the window labelled `main`. It does that
with named `jq` assignments rather than a deep merge, because a deep merge
replaces the whole `app.windows` array and the window would lose the URL it
opens and its size.

## Things that will confuse you once

* The desktop app will not bundle without the core binary staged at
  `tauri-app/binaries/godwinmix-<target triple>`. The workflow does it for you.
  If you build by hand, that is the step you forgot.
* The updater endpoint in the upstream config still points at GodwinMix's own
  release server. Your build will check it and find nothing that matches. Set
  your own endpoint and signing key in `tauri.conf.json` if you want in app
  updates, or ignore it and ship new installers.
* `gmx doctor` on a clean machine of each platform, before you publish, is
  worth more than any amount of reading. It exits non zero when something the
  default pipeline needs is missing.
* Your stream key must not be in `godwinmix.toml`. The preset ships
  placeholders; check that the file in the installer still has them.
* The core is Apache 2.0 and the `LICENSE` file goes with it, as does the
  licence of every plugin you bundle.

## Where the rest of the documentation is

* `docs/how-to/custom-build.md` in the GodwinMix repository, the local half of
  this.
* `docs/reference/presets.md`, for what a preset can carry.
* `docs/how-to/desktop-app.md`, for how the desktop app and the sidecar fit
  together.
