# Run a marketplace

A marketplace is a JSON file in a repository. It lists plugins and says where
each one comes from, so that `gmx plugin add ndi` works without anybody having
to remember a URL.

You would run one because your organisation wants its operators installing from
a list it controls, because you maintain a family of plugins that belong
together, or because you want a curated list for a kind of production that the
official one does not cover.

This page takes about fifteen minutes.

## Make one

A repository with `godwinmix-marketplace.json` at its root. That is the whole
requirement.

```json
{
  "name": "acme",
  "title": "Acme Broadcast",
  "description": "The plugins Acme supports on its channels.",
  "owner": "acme",
  "version": 1,
  "plugins": [
    {
      "name": "ndi",
      "source": "acme/gmx-ndi",
      "description": "NDI sources and outputs, with discovery.",
      "tier": "gold",
      "kinds": ["source", "output"],
      "license": "Apache-2.0",
      "repository": "https://github.com/acme/gmx-ndi",
      "versions": [
        {
          "version": "1.2.0",
          "api": 1,
          "platforms": ["linux-x86_64", "linux-aarch64", "macos-aarch64"],
          "signed": true
        }
      ]
    }
  ]
}
```

`name` is a slug. It is what appears in `gmx marketplace list` and what
`gmx marketplace remove` takes.

`source` is any of the forms `gmx plugin add` takes: `owner/repo`, a git URL, a
`cargo:`, `npm:` or `pypi:` spec, or a path. Every source in the file is parsed
when the marketplace is added, so a typo is caught by whoever added the
marketplace rather than by the operator who tried to install from it.

Every field is described in [the index format](../reference/index-format.md),
which is the same schema: the community index is a marketplace with more fields
filled in by its CI.

## Use it

    gmx marketplace add acme/plugins

That fetches
`https://raw.githubusercontent.com/acme/plugins/HEAD/godwinmix-marketplace.json`,
checks it, and caches it under `~/.godwinmix/marketplaces/acme.json`. It also
tries `index.json`, so a repository using that name works too.

A URL works, for a marketplace that is not on GitHub:

    gmx marketplace add https://mix.acme.example/marketplace.json

So does a path, which is how this repository's own marketplace is added and how
you test yours before pushing it:

    gmx marketplace add ./

Then:

    gmx plugin search ndi
    gmx plugin add ndi

`search` reads the cached copies, so it costs nothing and works on a show
network with no route out. `gmx marketplace refresh` fetches them all again.

## Where the list lives

`~/.godwinmix/marketplaces.json`, on the machine the core runs on. The core
reads it when it installs, so if your mixer is on another box, run
`gmx marketplace add` there. `gmx marketplace list` prints the path it read.

## Sign what you list

If your CI signs the assets it publishes, say so, and every install from your
marketplace is then checked against that identity rather than against nobody in
particular:

```json
"signing": {
  "identity_regexp": "^https://github.com/acme/plugins/.github/workflows/.+",
  "oidc_issuer": "https://token.actions.githubusercontent.com"
}
```

Without this block a signature is still checked against the bytes that arrived,
but nothing pins who made it. With it, a plugin signed by somebody else is
refused. [Trust and signing](../explanation/trust-and-signing.md) says what each
level actually proves.

## Pin your operators to it

An organisation that wants its mixers installing from its own list and nowhere
else puts this in the config on each machine:

```toml
[marketplaces]
only = ["acme"]
```

Then `gmx plugin add ndi` resolves through `acme` alone. A marketplace that is
added but not listed here is ignored, and a plugin only the community index
lists is not found. The operator can still install by explicit source, which is
deliberate: this is a default, not a lock, and a lock that can be worked around
with a URL is worth less than an honest default. If you want the harder
version, set `[plugins] allow_unsigned = false` beside it, and then only a
signed release installs at all.

## Keep it honest

Two plugins with the same name in two marketplaces are resolved by tier, and by
the order they were added when the tiers match. So an organisation that lists
its own build of `ndi` at `gold` gets its own build, and does not silently get
the community one.

A marketplace is a list, not a promise. If you list a plugin you did not write,
say so in its `description`, and do not list it at a tier it has not earned.
[The quality scale](../reference/quality-scale.md) says what each tier means,
and an operator reading `gold` on your list is entitled to assume it means what
it means everywhere else.

## The official marketplace

This repository is one. `godwinmix-marketplace.json` at the root lists the
first party plugins under `plugins/`, and it is generated from their manifests
by `tools/marketplace.py` so that the version in the listing is the version in
the manifest and a new plugin cannot be forgotten. Run it after adding a
plugin:

    python3 tools/marketplace.py

## Next

* [The index format](../reference/index-format.md), every field.
* [The quality scale](../reference/quality-scale.md).
* [Publish a plugin](publish-a-plugin.md), from the other side.
