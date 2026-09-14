# The index format

One schema, used by three things: the official marketplace
(`godwinmix-marketplace.json` at the root of this repository), the community
index (`index.json` in the `godwinmix-plugins` repository), and any marketplace
anybody else runs. The community index has more fields filled in because its CI
fills them; the reader is the same.

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
| `kinds` | array of string | no | What it provides: `source`, `output`, `filter`, `panel`, `service`, `device`, `preset`, `encoder`, `theme`. Searched, so a listing with them is found by somebody looking for an output. |
| `license` | string | no | An SPDX id. |
| `repository` | string | no | Where the code is. |
| `versions` | array | no | One entry per published version, newest last. |

### A version entry

| Field | Type | Required | What it is |
|---|---|---|---|
| `version` | string | yes | Semver. |
| `api` | integer | yes | The protocol level this version declares. The core picks the newest listed version whose `api` it can run, which is not always the newest version. |
| `platforms` | array of string | no | The platform triples this version has assets for: `linux-x86_64`, `linux-aarch64`, `linux-armv7`, `macos-aarch64`, `macos-x86_64`, `windows-x86_64`, `windows-aarch64`. |
| `signed` | boolean | no | Whether the CI signed the assets for this version. |
| `harness` | array of string | no | What the conformance harness found, one line per platform, written by the index CI. This is what the tier checks read and what the compatibility dashboard prints. |

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
