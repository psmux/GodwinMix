# The index format

One schema, used by three things: the official marketplace
(`godwinmix-marketplace.json` at the root of this repository), the community
index (`index.json` in
[psmux/godwinmix-plugins](https://github.com/psmux/godwinmix-plugins)), and any
marketplace anybody else runs. The community index has the gallery fields
filled in because its workflow reads each plugin's repository daily; the reader
is the same.

A mixer reads the community index with no setup: the Plugins page, a search or
an install of a bare name adds it the first time, and never before. See
[plugin.gallery](plugin-gallery.md).

`gmx marketplace add` fetches the document, `gmx plugin search` reads the cached
copy, and `gmx plugin add <name>` resolves a bare name through it.

The reader is `crates/godwinmix-host/src/marketplace.rs`. A machine readable
schema is at `index/schema.json`.

## The document

| Field | Type | Required | What it is |
|---|---|---|---|
| `name` | string | yes | A slug. Names the cache file, appears in `gmx marketplace list`, and is what `gmx marketplace remove` and `[marketplaces] only` take. |
| `title` | string | no | A human name, for the listing. |
| `description` | string | no | One sentence saying what this marketplace is for. |
| `owner` | string | no | Who runs it. |
| `version` | integer | no, default 1 | The schema version. A core that reads version 1 refuses a higher one and says to upgrade, rather than reading half of it. |
| `signing` | object | no | The sigstore identity this marketplace's CI signs with. See below. |
| `plugins` | array | yes | The listings. |

Unknown fields are kept rather than dropped, so a marketplace can carry
whatever its own tooling needs.

### `signing`

```json
"signing": {
  "identity_regexp": "^https://github.com/psmux/godwinmix-plugins/.github/workflows/.+",
  "oidc_issuer": "https://token.actions.githubusercontent.com"
}
```

| Field | What it is |
|---|---|
| `identity_regexp` | A regular expression the signing certificate's subject must match. For a GitHub Actions workflow that is the workflow's own URL. |
| `oidc_issuer` | The OIDC issuer. `https://token.actions.githubusercontent.com` for GitHub Actions. |

When a plugin is resolved through a marketplace that has this block, its
signature is checked against that identity. Without it a signature is checked
against the bytes that arrived and nothing more.

## A listing

| Field | Type | Required | What it is |
|---|---|---|---|
| `name` | string | yes | The plugin's own name, which is the namespace of every id it contributes. This is what `gmx plugin add <name>` matches. |
| `source` | string | yes | Where to get it. Any form `gmx plugin add` takes, listed below. Parsed when the marketplace is added, so a typo fails there. |
| `description` | string | no | One sentence. It is what a person and a model both read first, so write it for somebody deciding whether this is the plugin they want. |
| `tier` | string | no, default `custom` | `custom`, `bronze`, `silver` or `gold`. See [the quality scale](quality-scale.md). |
| `kinds` | array of string | no | What it provides, from the manifest's `[[provides]]` blocks: `source`, `output`, `filter`, `transition`, `encoder`, `service`, `device`, `panel`, `surface`, `preset`, `graphic`, `collection`. Searched, so a listing with them is found by somebody looking for an output. |
| `license` | string | no | An SPDX id. |
| `repository` | string | no | Where the code is. |
| `versions` | array | no | One entry per published version, newest last. |

### What a gallery shows

All optional, so a document without them reads as it always did. The Plugins
page draws a card from them; `gmx plugin add` reads none of them.

| Field | Type | What it is |
|---|---|---|
| `title` | string | A human name for the card, "NDI" for `ndi`. The name is shown when there is none. |
| `icon` | string | A square picture, a URL. |
| `thumbnail` | string | The card's picture, a URL, 16 by 9. |
| `screenshots` | array of string | Pictures for the detail view, URLs. Anything an `<img>` shows, so an animated GIF or WebP is here and a video never is. A mixer from before 0.3.10 reads only this. |
| `media` | array of object | Every picture and clip, in order, each `{"url": "...", "type": "image" \| "animation" \| "video"}`. The detail view's carousel. When it is missing the carousel is made from `screenshots`, a `.gif` read as an animation. |
| `preview` | string | The first animation or video in `media`, which a card plays while a mouse is over it. |
| `featured` | boolean | Put it in the Featured row at the top of the Plugins page. |
| `mixer` | string | The oldest GodwinMix that runs it, `0.3.10`. An older mixer shows the card as `Needs a newer GodwinMix` and offers no Install. |

### index.json and index2.json

A marketplace repository may carry `index2.json` beside `index.json`. A mixer
from 0.3.10 on tries `godwinmix-marketplace.json`, then `index2.json`, then
`index.json`, and reads the first it finds. A mixer from before 0.3.10 never
asks for `index2.json`. The community index uses that to keep plugins away
from mixers that cannot run them: `index2.json` has every listing, and
`index.json` leaves out every listing that names `mixer`.
| `stars` | integer | The repository's GitHub stars. |
| `downloads` | integer | Every download of every release asset, added together. |
| `updated` | string | When it last changed, RFC 3339. |
| `author` | string | Who makes it. The marketplace's `owner` is shown when there is none. |
| `homepage` | string | A web page about it. |
| `tags` | array of string | Words to search by. |
| `bundled` | boolean | It ships with GodwinMix. Its `source` may then be a path such as `./plugins/camera`, and installing it by name takes the copy beside the mixer with no download. |

The pictures are loaded by the browser from where they are; the core never
fetches one. The community index's workflow takes them from the plugin
repository's `gallery/` folder and falls back to the repository's social
preview: `icon.*` is the icon, `thumbnail.*` the card, and every other `.png`,
`.jpg`, `.jpeg`, `.gif`, `.webp`, `.mp4` or `.webm` goes into `media` in
natural name order (`screenshot-2` before `screenshot-10`), so `demo.gif`
comes first. A `.webp` is an animation only when it is animated.

### A version entry

| Field | Type | Required | What it is |
|---|---|---|---|
| `version` | string | yes | Semver. |
| `api` | integer | yes | The protocol level this version declares. The core picks the newest listed version whose `api` it can run, which is not always the newest version. |
| `platforms` | array of string | no | The platform triples this version has assets for: `linux-x86_64`, `linux-aarch64`, `linux-armv7`, `macos-aarch64`, `macos-x86_64`, `windows-x86_64`, `windows-aarch64`, and `any` when one asset runs on every computer (its name carries `-any`, `myplugin-1.0.0-any.zip`). A pure Python plugin ships that, since its asset holds source. A mixer from before 0.3.10 shows an `any` only release as not for this computer; per platform names work with every version. |
| `signed` | boolean | no | Whether the CI signed the assets for this version. |
| `harness` | array of string | no | What the conformance harness found, one line per platform, written by the index CI. This is what the tier checks read and what the compatibility dashboard prints. |

### The two kinds with no binary

A `graphic` and a `collection` are files, not programs. A graphic is an OGraf
template (a manifest and a web component); a collection is a set of scenes with
their assets. Neither has a process, neither declares `[run]`, and neither is
built per platform.

That changes what a listing for one means:

* `platforms` on a version entry is about where a binary runs. A graphic or a
  collection runs wherever the core does, so the field is left out rather than
  listing all seven triples.
* `harness` is the conformance harness's per platform result. A provide with no
  process has nothing to spawn, so `gmx plugin test` checks the manifest, every
  schema and every `SKILL.md` and says so. A listing of these kinds is not held
  to a per platform harness matrix.
* `signed` still means what it means everywhere: the CI signed the assets.

A plugin that ships both, a graphics host with its templates for example, is
listed by everything it provides (`["service", "graphic"]`) and is held to the
rules of the kinds that do have a binary.

## Source forms

`source` is one of these. The parser is in
`crates/godwinmix-host/src/sources/mod.rs`.

| Form | Example | Notes |
|---|---|---|
| GitHub release | `psmux/gmx-ndi` | The default. `@1.2.0` pins a version; without one the latest release wins. |
| git | `https://gitlab.com/x/y.git` | `#branch-or-tag` after it pins a reference. Built on the installing machine using the manifest's `[build]` section. |
| cargo | `cargo:gmx-ndi` | `@0.3.1` pins. Needs a Rust toolchain on the installing machine. |
| npm | `npm:@scope/gmx-chat` | A leading `@` is a scope, not a version, so `npm:@x/y@1.0.0` means what it looks like. |
| PyPI | `pypi:gmx-director` | The source distribution, not a wheel: that is where `gmx-plugin.toml` is. |
| OCI | `oci:ghcr.io/x/gmx-ai-vision:1.0` | Parsed and recognised. Installing one is refused today with the next step; a container plugin is placed on a node, which is not in this release. |
| path | `./plugins/ndi` | Relative to the marketplace document for a local marketplace. Always `custom` for trust purposes. |

## A whole document

This is the shape a signed index takes. The community index today has no
`signing` block and says `"signed": false` on every version, because its
workflow does not build or sign anything yet; its README says so.

```json
{
  "name": "godwinmix-plugins",
  "title": "The GodwinMix community index",
  "description": "Every plugin the bot has checked.",
  "owner": "psmux",
  "version": 1,
  "signing": {
    "identity_regexp": "^https://github.com/psmux/godwinmix-plugins/.github/workflows/.+",
    "oidc_issuer": "https://token.actions.githubusercontent.com"
  },
  "plugins": [
    {
      "name": "clock",
      "source": "psmux/gmx-clock",
      "description": "Draws the current time, large, on a solid background.",
      "tier": "bronze",
      "kinds": ["source"],
      "license": "MIT",
      "repository": "https://github.com/psmux/gmx-clock",
      "versions": [
        {
          "version": "0.1.0",
          "api": 1,
          "platforms": ["linux-x86_64", "macos-aarch64"],
          "signed": true,
          "harness": ["linux-x86_64: 8/8", "macos-aarch64: 8/8"]
        }
      ]
    }
  ]
}
```

## How a name is resolved

1. Every marketplace the operator added is read from its cache, narrowed to
   `[marketplaces] only` when that is set.
2. Listings whose `name` matches are collected.
3. They are sorted by tier, highest first, and by the order the marketplaces
   were added when tiers match.
4. The winner's `source` is what gets installed.

So an organisation that lists its own build of a plugin at `gold` gets its own
build, and a plugin that appears on the community index at `bronze` and on the
official one at `gold` comes from the official one.

## Validating one

    python3 index/bot/validate.py

That is the same script the index bot runs on a pull request. It checks the
schema, the tier requirements, and the harness results, and names the failing
check when it refuses.

## See also

* [Run a marketplace](../how-to/run-a-marketplace.md)
* [Publish a plugin](../how-to/publish-a-plugin.md)
* [The quality scale](quality-scale.md)
* [Trust and signing](../explanation/trust-and-signing.md)
