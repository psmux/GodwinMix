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

The list every mixer reads is
[psmux/godwinmix-plugins](https://github.com/psmux/godwinmix-plugins). A fresh
mixer knows it already, so once your plugin is on it, it shows on the Plugins
page of every mixer and `gmx plugin add <name>` finds it.

Open a pull request there adding one small file, `listings/<name>.toml`:

```toml
name = "thing"
repo = "you/gmx-thing"
title = "Thing"            # optional, the name on the card
```

`name` is the plugin's name from your manifest and the file's name. That is
the whole entry. A workflow in that repository reads your repository once a day
and writes what it finds into `index.json`:

* the description: the `description` in `gmx-plugin.toml`, else the first
  paragraph of your README, else the repository's own description
* the kinds, from your manifest's `[[provides]]` blocks
* the newest release, the platforms its asset names carry, and every
  download of every release added together
* the repository's stars, licence, topics and when it last changed

It runs on the pull request too, and names the problem if it cannot read your
repository or your manifest names a different plugin.

### Pictures for the card

Put them in a `gallery` folder at the root of your repository:

| File | What it is for |
|---|---|
| `gallery/icon.png` | a square icon beside the name. 256 by 256 is plenty |
| `gallery/thumbnail.png` | the card's picture, 16 by 9. 1280 by 720 is plenty |
| `gallery/screenshot-1.png`, `-2`, and on | the detail view, in name order |

PNG, JPEG, WebP and GIF all work. With no thumbnail the first screenshot is the
card's picture, and with no screenshots the repository's social preview is. With
none of them the card shows an icon for the plugin's kind. The browser loads
the pictures straight from GitHub; the mixer never fetches them.

### What the list does not do yet

It does not build your plugin, run the conformance harness or sign anything.
Every listing in it is `custom` and every version says `"signed": false`, and
the list has no `signing` block, so a mixer checks no signature against it. A
mixer set to `[plugins] allow_unsigned = false` will not install from it. The
plugins that ship with GodwinMix are listed there too, marked `bundled`, and are
installed from the copy beside the mixer.

## 4. Publishing the assets yourself

The list does not build anything yet, so for now you build your own release
assets, and your listing is at the `custom` tier. `gmx plugin add` installs them and labels them
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
