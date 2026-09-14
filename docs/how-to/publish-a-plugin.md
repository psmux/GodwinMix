# Publish a plugin

You have a plugin that passes `gmx plugin test`. This page gets it to the point
where somebody you have never met types `gmx plugin add <your plugin>` and it
works on their machine.

It takes about half an hour, most of which is waiting for CI.

## What people will install

A plugin is installed from one of seven places, and you choose which by
choosing where you publish. The first one is the one to aim for.

| You publish | They type | What happens on their machine |
|---|---|---|
| a GitHub release with an asset per platform | `gmx plugin add you/gmx-thing` | the asset for their platform is downloaded, its signature checked, and unpacked. No compiler, no toolchain. |
| a git repository with a `[build]` section | `gmx plugin add https://host/you/thing.git` | it is cloned and built there, so they need your language's toolchain |
| a crate | `gmx plugin add cargo:gmx-thing` | the published source is fetched and `cargo install` builds it |
| an npm package | `gmx plugin add npm:@you/gmx-thing` | `npm pack`, then `npm install --omit=dev` after it lands |
| a PyPI package | `gmx plugin add pypi:gmx-thing` | the source distribution is fetched and a virtual environment is built for it |

A release with prebuilt assets is the kindest of these, because it is the only
one that works on a machine with nothing installed but GodwinMix. The rest of
this page is about getting there without owning five machines.

## 1. Tag a version

`version` in `gmx-plugin.toml` is semver and required. Tag the repository to
match:

    git tag v0.1.0
    git push --tags

`v0.1.0` and `0.1.0` are both understood when somebody pins a version with
`gmx plugin add you/gmx-thing@0.1.0`.

## 2. Add the topic

Put the topic `godwinmix-plugin` on your repository, in the About panel on
GitHub. That is the whole of the discovery mechanism before an index entry
exists, and it is what people search when they want to know whether the thing
they need has been written already.

## 3. Get it listed

Open a pull request against the index repository adding an entry to
`index.json`. [The index format](../reference/index-format.md) is one page and
the entry is about twelve lines. A bot checks it: the manifest validates, the
source resolves, the platforms are real ones, and the conformance harness
passes. If a check fails the bot says which one, and the pull request waits for
you rather than for a person.

Once it is merged, the index CI takes over and this is what you stop having to
do yourself:

* It builds your plugin from the tagged source on Linux x86_64, Linux aarch64
  (a Raspberry Pi class runner), macOS arm64, macOS x86_64 and Windows x86_64.
* It runs the conformance harness on each one.
* It signs every asset with sigstore, keyless, tied to the CI's own identity.
  You need no signing key, no certificate, and no Apple developer account.
* It publishes the assets and their `.sigstore.json` bundles as release assets
  on your repository, through a bot, or on the index's own release if you would
  rather it did not push to yours.
* It writes the badge for your README and updates the public compatibility
  dashboard.

The signing is worth dwelling on, because it is the thing that stopped people
writing OBS plugins. Three platform builds, notarisation, and 99 dollars a year
to Apple, per author, to publish something you are giving away. A GodwinMix
plugin is a separate process rather than a library loaded into an app bundle,
so the rules that make an in process plugin a notarisation problem do not
apply: the core is notarised once, and it runs your binary the way it runs any
other program. What `gmx plugin add` checks is the index CI's signature. See
[trust and signing](../explanation/trust-and-signing.md) for the whole of it.

## 4. Publishing the assets yourself

You do not have to use the index CI. If you build your own binaries, list them
at the `custom` tier and say so. `gmx plugin add` installs them and labels them
`custom, unreviewed` wherever an operator can see them, which is honest rather
than punitive: an operator who trusts you installs it anyway.

If you do build your own, the naming matters, because that is how the installer
picks the right one:

    gmx-thing-0.1.0-linux-x86_64.tar.gz
    gmx-thing-0.1.0-linux-x86_64.tar.gz.sigstore.json
    gmx-thing-0.1.0-linux-aarch64.tar.gz
    gmx-thing-0.1.0-macos-aarch64.tar.gz
    gmx-thing-0.1.0-windows-x86_64.zip

The rule is that the platform triple appears in the file name. The triples are
`linux-x86_64`, `linux-aarch64`, `linux-armv7`, `macos-aarch64`,
`macos-x86_64`, `windows-x86_64` and `windows-aarch64`. Nothing is guessed from
`amd64` or `x64`.

Prefer `.tar.gz`. A zip loses the executable bit on Unix, and then the plugin
starts with "permission denied" and nothing says why.

The signature file sits beside the asset with `.sigstore.json` on the end. If
you sign with cosign yourself:

    cosign sign-blob --yes \
      --bundle gmx-thing-0.1.0-linux-x86_64.tar.gz.sigstore.json \
      gmx-thing-0.1.0-linux-x86_64.tar.gz

Without a signature file the plugin still installs. It is labelled
`custom, unreviewed`, and an operator who has set `[plugins] allow_unsigned =
false` will not get it at all.

## 5. Declare the platforms honestly

`platforms` in `gmx-plugin.toml` is what the installer checks before it copies
anything:

    platforms = ["linux-x86_64", "linux-aarch64", "macos-aarch64", "windows-x86_64"]

A plugin that lists a platform it has never run on produces a refusal an
operator cannot act on. A plugin that lists only what it has is refused
quickly, with the list printed, which is the better failure.

## 6. Say what api level you wrote against

    api = 1

The core runs a plugin whose `api` is inside its own supported range. Outside
it, the install is refused with the version of GodwinMix that would run it,
rather than a crash at launch:

    gmx-thing 2.0.0 needs api 2 and was not installed. GodwinMix 0.4.0 or later
    speaks api 2; this core is 0.2.0, which speaks api 1.

`api_level` goes up only when something is added, and only at an announced
major does the oldest supported level move, at most once a year, with the
previous level supported for a year after. You are not signing up for a rewrite
every release.

## 7. Release a new version

Tag it. The index CI notices the tag, rebuilds, retests, resigns and updates
the entry. An operator then gets it with:

    gmx plugin update thing

That command installs the new build beside the old one and gives it ten seconds
to say hello. A build that does not start is rolled back and the operator keeps
the version that worked. It costs you nothing, and it means a bad release is an
inconvenience rather than an outage on somebody's Sunday service.

## What to put in the repository

The templates write all of this, and `gmx plugin new` writes the templates:

* `gmx-plugin.toml`, the manifest.
* `SKILL.md` per kind, which is what an AI agent reads to know when to use your
  plugin. Required from bronze upward.
* `AGENTS.md`, how to build and test this plugin, for a coding agent extending
  it.
* A settings schema per provide, with an `examples` array on every field.
  Required from silver upward, and the reason is that the schema is the only
  settings UI your plugin gets.
* `tests/transcript.jsonl`, so `gmx plugin test --offline` runs in your own CI
  with no core and no GStreamer.
* A README with the harness badge in it.

## Next

* [The index format](../reference/index-format.md), every field in an entry.
* [The quality scale](../reference/quality-scale.md), what bronze, silver and
  gold ask for.
* [Trust and signing](../explanation/trust-and-signing.md), what is checked and
  what is not.
* [Run a marketplace](run-a-marketplace.md), if you would rather publish a list
  of your own.
