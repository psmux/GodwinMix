# The marketplace methods

A marketplace is a repository with `godwinmix-marketplace.json` (or
`index.json`) at its root, listing plugins and where each one comes from. It is
not a service: anybody can host one, and this core reads whichever ones the
operator added.

Until a mixer has one, `plugin.search` answers nothing and `plugin.add camera`
by bare name cannot resolve, because there is nowhere to look the name up. That
used to be fixable only with `gmx marketplace add` in a shell. These four
methods put the same store behind the protocol, so a web page or a desktop app
can do it with a button.

All four are `Scope::Admin`. Adding a marketplace decides where this machine
will fetch code from, which is not an operator's decision to make mid show.

| Method | HTTP | What it does |
|---|---|---|
| `marketplace.list` | `GET /api/v1/marketplaces` | what this machine knows, and what the project recommends |
| `marketplace.add` | `POST /api/v1/marketplaces` | fetch one, check it, cache it, record it |
| `marketplace.remove` | `DELETE /api/v1/marketplaces/{id}` | forget one and its cached listing |
| `marketplace.refresh` | `POST /api/v1/marketplaces/refresh` | fetch every added one again |

The list lives in `~/.godwinmix/marketplaces.json` and each document is cached
at `~/.godwinmix/marketplaces/<name>.json`. The cache is a copy, never the
truth: a search reads what is already there, so it costs nothing and works on a
show network with no route out.

## marketplace.list

```
marketplace.list {}
GET /api/v1/marketplaces
```

```json
{
  "marketplaces": [
    {
      "name": "godwinmix",
      "title": "GodwinMix official",
      "description": "The plugins the project maintains.",
      "owner": "psmux",
      "source": "psmux/godwinmix",
      "url": "https://raw.githubusercontent.com/psmux/godwinmix/HEAD/godwinmix-marketplace.json",
      "plugins": 9,
      "fetched": 1758153600,
      "signs": true,
      "consulted": true
    }
  ],
  "store": "/home/you/.godwinmix/marketplaces.json",
  "only": [],
  "recommended": [
    {
      "name": "godwinmix",
      "source": "psmux/godwinmix",
      "title": "GodwinMix official",
      "description": "The plugins the project maintains…",
      "added": true,
      "first_party": true
    }
  ]
}
```

| Field | What it is |
|---|---|
| `signs` | the document names the sigstore identity its CI signs with, so a plugin resolved through it is verified against that identity rather than against nobody in particular |
| `consulted` | whether `[marketplaces] only` lets this core read it. A marketplace added before a pin was set is still listed, and this says it is being skipped |
| `problem` | the cached copy could not be read. The row stays, because a marketplace the operator added and cannot see is worse than one that says why it is not working |
| `only` | the pinned list, empty when nothing is pinned |
| `recommended` | the marketplaces the project runs, each with `added` |

`recommended` is answered whether or not anything is configured, so a surface
can offer "Add the GodwinMix marketplace" as one button on a fresh machine.
Nothing is added on your behalf: see [what a fresh core does](#what-a-fresh-core-does).

## marketplace.add

```
marketplace.add {"source": "psmux/godwinmix"}
POST /api/v1/marketplaces   {"source": "psmux/godwinmix"}
```

`source` is `owner/repo`, a URL to the document or to the repository root, or a
path to a directory with a marketplace document in it.

This one fetches over the network, so it answers with a
[task handle](tasks.md) and the work carries on:

```json
{ "task_id": "marketplace-add-1", "poll_interval_ms": 1000, "state": "running", "source": "psmux/godwinmix" }
```

`task.get` answers with the `MarketplaceRecord` once it lands. The document is
parsed and checked before anything is written: a file that is not a marketplace,
or one declaring a schema version this core does not read, is refused with the
reason and nothing is recorded.

A core pinned with `[marketplaces] only` refuses a marketplace that pin would
never read, and says which list it is pinned to. The name is only known once
the document has been read, so the fetch happens first and the record is put
back; the store is left exactly as it was.

## marketplace.remove

```
marketplace.remove {"id": "godwinmix"}
DELETE /api/v1/marketplaces/godwinmix
```

`name` is accepted for `id`. Answers immediately:

```json
{
  "removed": "godwinmix",
  "source": "psmux/godwinmix",
  "note": "plugins installed from `godwinmix` stay installed and keep working…"
}
```

Nothing is uninstalled. What stops working is resolving a bare name that this
marketplace was the only source of, so `plugin.add` will want the source
written out from then on.

## marketplace.refresh

```
marketplace.refresh {}
POST /api/v1/marketplaces/refresh
```

Another task handle. The result is one row per marketplace:

```json
{
  "refreshed": [
    { "name": "godwinmix", "plugins": 9 },
    { "name": "godwinmix-plugins", "plugins": 0, "problem": "asking https://… timed out" }
  ]
}
```

One that cannot be reached keeps its cached copy, so a search goes on working.

## What a fresh core does

Nothing. A core with no marketplaces configured adds none by itself.

Seeding the first party one at startup was considered and turned down. It would
put a network fetch in the boot path of every machine, including the ones on a
venue network with no route out and the ones an organisation deliberately
pinned or air gapped, and it would record a decision about where this machine
fetches code from that nobody made. `marketplace.list` naming the recommendation
costs one click instead, and the click is the operator's.

## Trust

These methods change nothing about what is checked at install time. A plugin
resolved through a marketplace that declares `signing` is verified against that
identity; `[plugins] allow_unsigned = false` still refuses anything unsigned;
`[marketplaces] only` still narrows what is read. Adding a marketplace adds a
place to look, never a permission.

[Trust and signing](../explanation/trust-and-signing.md) says what each level
proves. [Run a marketplace](../how-to/run-a-marketplace.md) says how to publish
one.
