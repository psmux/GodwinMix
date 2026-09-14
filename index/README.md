# godwinmix-plugins

The community index for [GodwinMix](https://github.com/psmux/godwinmix) plugins.

It is a repository with an `index.json` in it. There is no service behind it and
no database. A plugin author opens a pull request that adds an entry, a bot
checks the entry, CI builds and tests the plugin on five platforms, signs what
it built, and writes the result back into the same file. An operator adds the
whole thing with one command:

```
gmx marketplace add psmux/godwinmix-plugins
gmx plugin search ndi
gmx plugin add ndi
```

The search reads a cached copy, so it costs nothing and works on a show network
with no route out. See [compatibility.md](compatibility.md) for what is listed
and what passed where.

## Listing a plugin

1. Fork this repository.
2. Add an entry to the `plugins` array in `index.json`. Copy an existing one and
   change it. Keep the array readable; the core sorts by tier itself.
3. Open a pull request. Use the template, which is the checklist below.
4. The bot runs. It takes under a minute and it tells you the name of any check
   that refused the listing.
5. A maintainer reads the entry and the repository it points at. Most listings
   are merged within a few days.
6. On merge, the build matrix runs. Ten to fifteen minutes later your plugin has
   signed assets, a badge, and a row in the dashboard.

The smallest entry that gets past the bot at `custom`:

```json
{
  "name": "clock",
  "source": "psmux/gmx-clock",
  "description": "A source that draws the current time, large, on a solid background.",
  "tier": "custom",
  "kinds": ["source"],
  "license": "MIT",
  "repository": "https://github.com/psmux/gmx-clock"
}
```

For bronze and above, add `skill`, `policy` and `versions`. `schema.json` is a
JSON Schema for the whole document with a description on every field, so an
editor that understands `$schema` will complete the entry as you type it.

### What `source` may be

The index takes any form the core parses, except a path:

| Form | Example |
|---|---|
| a GitHub release | `psmux/gmx-ndi`, or `psmux/gmx-ndi@1.3.1` to pin |
| a git repository | `https://git.example.org/x/gmx-thing.git` |
| a crate | `cargo:gmx-ndi` |
| an npm package | `npm:@example/gmx-chat` |
| a PyPI package | `pypi:gmx-director` |
| a container image | `oci:ghcr.io/example/gmx-thing:1.0` |

A path such as `./my-plugin` works in `gmx plugin add` and is refused here. It
resolves on the machine it was typed on and nowhere else.

## What the bot checks

`bot/validate.py` is Python 3 with no dependencies. Run it before you open the
pull request:

```
python3 bot/validate.py --entry my-plugin
python3 bot/validate.py --entry my-plugin --plugin-dir ../my-plugin
```

It prints one line per check and exits non zero naming the check that refused
the listing. The whole list, which is the `CHECKS` table at the bottom of the
file:

| Check | What it wants |
|---|---|
| `document.shape` | the document has a name and a plugins array |
| `document.version` | the schema version is one this core reads |
| `document.signing` | the signing block names an identity and an issuer |
| `document.names` | no two entries share a name |
| `document.cores` | the core releases the dashboard reports against |
| `entry.name` | the name is a slug, because it is the namespace of every id |
| `entry.source` | the source is one of the forms above |
| `entry.tier` | the tier is custom, bronze, silver or gold |
| `entry.kinds` | every kind is one the core registers |
| `entry.metadata` | description, licence and repository are there |
| `entry.versions` | every version has a semver and an integer `api` |
| `entry.platforms` | every platform is one of the seven triples |
| `entry.harness` | every harness line reads `platform: passed/total` |
| `entry.bronze` | a release, a SKILL.md, one platform passing |
| `entry.silver` | every declared platform passing |
| `entry.gold` | maintainers, evals, docs reviewed, official |
| `entry.policy` | network use, secrets and telemetry are declared |
| `entry.manifest` | the checked out manifest agrees and its entry point exists |
| `entry.harness-run` | `gmx plugin test` passes on the checked out source |

The last two need a checkout, so they run in CI and are skipped when you run the
bot against `index.json` alone. `GMX` names the core binary if it is not on
`PATH` as `gmx`.

The bot tests itself. `python3 bot/validate.py --self-test` runs the documents in
`bot/fixtures/`, each named for what it should do, and asserts that each one is
accepted or refused by the check its name promises. `bot/fixtures/broken-plugin/`
is a plugin directory whose manifest parses and whose entry point is missing; it
exists so the refusal path is tested and not only the happy one.

## What CI does

`.github/workflows/listing.yml`, in order:

* On a pull request touching `index.json`, it runs the bot, checks both JSON
  files parse, and checks `compatibility.md` is what the generator writes. Under
  a minute.
* On merge to `main`, or a manual run naming one plugin, it works out which entry
  changed and checks out that plugin at the tag its newest version names.
* Five runners build it and run `gmx plugin test` on it: `ubuntu-latest`,
  `ubuntu-24.04-arm`, `macos-latest`, `macos-13`, `windows-latest`. Each one
  packages `<plugin>-<version>-<platform>.tar.gz`, a `.zip` on Windows. Ten to
  fifteen minutes, longer for a plugin that compiles something large.
* One job signs every asset with sigstore, keyless, writing
  `<asset>.sigstore.json` beside each, verifies what it just signed, and
  publishes the assets and the bundles as release assets on this repository. A
  minute or two.
* One job writes `badges/<plugin>.json` in the shields.io endpoint shape onto the
  `badges` branch, so `https://img.shields.io/endpoint?url=...` renders the tier.
* One job regenerates `compatibility.md` and commits it when it moved.

The signing identity is in `index.json` under `signing`, and it names this
repository's workflow. That is what a core verifies against when it installs
from this index, which is also why the signed assets live on releases here
rather than on the plugin's own repository.

## The quality scale

Four tiers. The core reads the tier off the entry and sorts by it; an operator
sees it before anything is installed.

**custom**: listed by topic or added by path, no checks. What the operator sees:
"unreviewed, runs with the permissions you give it".

**bronze**: the manifest validates; the conformance harness passes on at least
one platform; a SKILL.md is present; there is a release with a version. Gets a
badge and is listed.

**silver**: the harness passes on all declared platforms; the settings schema has
examples for every field; tools have annotations; no leaks after 30 add and
remove cycles; a code owner responds within a month. Sorted above bronze.

**gold**: maintained by two or more people or by the project; an eval suite with
recorded shows; documentation reviewed; on the official marketplace. Preloaded
in presets.

The bot enforces the mechanical half: the harness results, the SKILL.md, the
release, the platform coverage, the maintainers, the eval suite and the review
date. The rest is what a reviewer reads, and the harness itself covers more than
the bot can see from the index. Check 4 configures the plugin with every example
in its settings schema, and check 5 asserts it leaves no descriptors and no
directories behind, which is where the schema examples and the leak requirement
are actually tested.

A tier is not permanent. A listing that stops passing drops to the tier it still
earns, and the dashboard says so on the next run.

## The listing policy

Five rules. They apply to every tier, including `custom`.

**No obfuscation.** The source a listing points at is the source that runs.
Minified output is fine when the sources that produced it are in the same
repository and the build is reproducible from them. A binary blob with no
buildable source, a packed script, or a loader that fetches the real payload at
runtime is refused.

**No telemetry without disclosure.** If the plugin reports anything anywhere,
`policy.telemetry` says what is collected, where it goes and how it is switched
off. Silence means none, and is taken as a promise.

**Declared network use.** `policy.network` lists every host or protocol the
plugin talks to, one line each, in plain words. An empty array means it never
touches the network. A source that reaches an address the listing does not
mention is a policy break, not a bug.

**Declared secrets.** `policy.secrets` names every credential the plugin reads
and where each one is sent. A plugin reads the keys the operator gave it and
nothing else: no scanning the home directory, no reading another plugin's
config, no environment sweep.

**Nothing that touches the programme without being asked.** A plugin does its own
job. It does not take, it does not change the layout, and it does not reconfigure
other plugins unless the operator drove it there.

When a listing breaks the policy:

1. An issue is opened naming the rule and the evidence, and the maintainers
   are asked to answer.
2. The tier drops to `custom` the same day, so nobody installing it is told
   anything was checked. The badge changes with it.
3. If it is a disclosure problem and the author fixes the declaration, the
   listing goes back to the tier its harness results earn.
4. If it is deliberate, the entry is removed from `index.json` and the plugin
   name is recorded so the same name cannot be relisted quietly. Removal stops
   `gmx plugin search` finding it and `gmx plugin add <name>` resolving it. It
   does not reach into an installation that already exists; an operator who
   installed it stays in control of their own machine, which is the point of the
   whole design.
5. Anything that reads credentials it was not given, or ships a payload it did
   not declare, is removed first and discussed afterwards, and the signed assets
   for it are deleted from the releases here.

Report a suspect listing by opening an issue, or privately to the address in
`SECURITY.md` in the core repository if disclosing it publicly would put shows
at risk.

## Running your own index

An organisation that wants its own list of approved plugins forks this
repository and changes four things:

1. `name` in `index.json`. It names the cache file and it is what
   `gmx marketplace remove` takes, so make it yours.
2. `title`, `description` and `owner`.
3. `signing.identity_regexp`, so it points at your fork's workflow rather than
   this one. Until you do, the assets your CI signs will not verify.
4. The `plugins` array. Empty it and add your own.

Then:

```
gmx marketplace add your-org/your-index
```

Operators can be pinned to a fixed list with `[marketplaces] only` in
`godwinmix.toml`, and nothing else is consulted after that. An index is also
readable from a plain URL or a directory on disk, which is how an air gapped
site runs one on a file share.

## The files here

| File | What it is |
|---|---|
| `index.json` | the index itself, the only file a core reads |
| `schema.json` | a JSON Schema for it, draft 2020-12, one description per field |
| `compatibility.md` | the dashboard, generated, never edited by hand |
| `bot/validate.py` | the pull request bot |
| `bot/dashboard.py` | the dashboard generator |
| `bot/fixtures/` | the documents the self test runs, and one broken plugin |
| `.github/workflows/listing.yml` | validate, build, sign, badge, dashboard |
| `.github/PULL_REQUEST_TEMPLATE.md` | the checklist a listing satisfies |

## Licence

The index and the bot are MIT. Each listed plugin carries its own licence, named
in its entry.
