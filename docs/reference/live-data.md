# Live data

The `feed.*` methods, their events, the formats a feed is read in and the
limits on it. How to use them is in
[Show live data on air](../how-to/show-live-data.md); what `select` and
`template` take is in [feed paths and templates](feed-paths.md). The schemas
are in `protocol.json`.

## Where it runs

In the show's own process, in the control plane beside the method handlers,
as one tokio task per feed. Not in a sidecar plugin: the binary already links
reqwest, tokio-tungstenite and the runtime, and a sidecar would be a second
process holding its own copies of all three plus a token and a connection
back, to make calls that are a function call here. A plugin also cannot add
methods to the protocol, and these have to be in it.

Nothing runs on the mixer thread or a GStreamer streaming thread. A binding
writes by running the handler of `source.set`, `scene.apply_graphic` or
`scene.params.set` from the same method table `/rpc` serves, so it can do
nothing a client could not. It does not count as an operator for the
watchdog in [safety](safety.md), and its writes are not in the session log.

A feed that hangs is held by its own timeout in its own task; no other feed
waits for it. A show with no feeds has no task, no timer and no HTTP client.

## Methods

| Method | Scope | MCP tool | Does |
|---|---|---|---|
| `feed.list` | read | `list_feeds` | every feed and binding with its state |
| `feed.test` | operate | `test_feed` | fetch once and show what a selection picks; stores nothing |
| `feed.add` | operate | `add_feed` | add a feed and start reading it |
| `feed.set` | operate | `set_feed` | change a feed's address, format, interval, timeout or headers |
| `feed.pause` | operate | `pause_feed` | stop or start reading a feed |
| `feed.refresh` | operate | `refresh_feed` | fetch a polled feed now |
| `feed.remove` | operate | `remove_feed` | forget a feed, its sealed headers and its bindings |
| `feed.binding.add` | operate | `bind_feed` | bind a value in a feed to a target |
| `feed.binding.set` | operate | `set_feed_binding` | change a binding's selection or target |
| `feed.binding.pause` | operate | `pause_feed_binding` | hold a binding, or let it write again |
| `feed.binding.remove` | operate | `remove_feed_binding` | forget a binding |

Over REST each is `POST /api/v1/feed/<name>` (`GET` for `feed.list`), with
the dots of a binding method as slashes: `/api/v1/feed/binding/add`. The MCP
tools are behind `search_tools`.

`feed.test` is operate rather than read because it makes the mixer fetch an
address. It is not a mutating call: it keeps nothing, takes no
`idempotency_key` and is not in the session log.

### `feed.add`

| Field | Type | Default | Meaning |
|---|---|---|---|
| `id` | string | | a slug: lower case letters, digits and dashes, starting with a letter. Never changes |
| `address` | string | | `http://`, `https://`, `ws://` or `wss://`. Anything else is refused |
| `format` | string | `auto` | `auto`, `json`, `rss`, `csv`, `text` or `sse` |
| `interval_s` | number | 30 | seconds between fetches of a polled feed, 5 to 86400 |
| `timeout_s` | number | 10 | seconds one fetch may take, 1 to 60 |
| `headers` | object | none | sent with every request; values are sealed |
| `paused` | boolean | false | added without being read |

Unknown fields are refused. The answer is the feed's status.

### `feed.set`

`id` and any of `address`, `format`, `interval_s`, `timeout_s`, `headers`.
Only what is named changes. `headers` replaces the whole set; a value of
`"__secret__"` keeps the one stored under that name, and a name left out is
forgotten. The feed's task starts again with the new settings.

### `feed.pause`, `feed.refresh`, `feed.remove`

`{"id": "..."}`, with `"paused": false` on `feed.pause` to start it again
(`true` when absent). `feed.refresh` on a paused feed is refused with
`-32001` and says to resume it. `feed.remove` answers
`{"removed": id, "bindings": [ids]}`; what the bindings wrote stays on air.

### `feed.test`

Either `id`, for a feed that exists, or `address` with `format`, `headers`
and `timeout_s` for one that does not. Then any of `select`, `template`,
`limit`, `join` to see what a binding with them would write. For a feed that
exists the document it last read is used, unless `fresh` is true or it has
read none yet. For a websocket or an event stream the first message is
waited for, up to the timeout.

| Answer | Meaning |
|---|---|
| `format` | what it was read as |
| `bytes`, `took_ms` | size of the body, and how long the test took |
| `keys` | the keys at the top of the document |
| `preview` | the document with lists cut to five elements and strings to 200 characters |
| `paths` | every path to a value, up to 200, a list gone into by its first element as `[]`, each with an `example` |
| `selected` | what `select` picked, cut like `preview` |
| `value` | what a binding would write |

A selection that picks nothing is refused with `-32602` and `data.field`
`select`, `data.stopped_at`, `data.keys_there`, `data.top_level_keys` and
`data.paths`. A feed that cannot be read is refused with `-32001`, the reason
and `data.retryable: true`.

### `feed.binding.add`

| Field | Type | Meaning |
|---|---|---|
| `id` | string | a slug. Made from the target when absent: `<source>-<param>`, the field, or the scene parameter, with a number if taken |
| `feed` | string | the feed it reads |
| `select`, `template`, `limit`, `join` | | see [feed paths and templates](feed-paths.md) |
| `to` | object | the target, one of the three below |
| `paused` | boolean | added without writing |

| `to` | Writes through |
|---|---|
| `{"source": "crawl", "path": "params.items"}` | `source.set` with that param. The path starts `params.`; a nested one (`params.fields.headline`) is merged into what the source has under its first key |
| `{"graphic": "ograf/lower-third", "field": "name", "item": "speaker strap"}` | `scene.apply_graphic` with `values: {field: value}` and no play. `item` is optional |
| `{"scene_param": "speaker"}` | `scene.params.set` with `values: {speaker: value}` |

A source that does not exist is refused with `-32004` and the ids that do.
When the feed has been read, a selection that picks nothing is refused as in
`feed.test`, and a good one is written at once.

### `feed.binding.set`, `feed.binding.pause`, `feed.binding.remove`

`feed.binding.set` takes `id` and any of `select`, `template`, `limit`,
`join`, `to`; an empty `template` or `join`, or a `limit` of 0, takes it
away. The value is written again at once. `feed.binding.pause` takes `id`
and `paused`; started again it writes what the feed holds now.
`feed.binding.remove` answers `{"removed": id}`.

## What `feed.list` answers

`{"feeds": [...], "bindings": [...]}`.

A feed: every field of `feed.add` (header values as `"__secret__"`), and

| Field | Meaning |
|---|---|
| `state` | `starting` (not read yet, or connecting), `ok`, `failing` or `paused` |
| `kind` | `polled`, `websocket` or `sse` |
| `last_fetch` | when something was last read, RFC 3339 |
| `last_change` | when what was read last differed from what came before |
| `last_error` | why the last attempt failed, and what to do about it |
| `failures` | attempts in a row that failed |
| `fetches` | reads since the core started, `304`s included |
| `not_modified` | fetches the server answered `304 Not Modified` |
| `bytes` | size of the last body read |

A binding: every field of `feed.binding.add`, and `value` (what it last
wrote), `last_write`, `writes` (since the core started) and `last_error`.

## Events

| Event | Payload | Sent |
|---|---|---|
| `event/feed.failed` | `id`, `binding`, `error`, `failures` | on the first failure in a row of a feed (`binding` null) or of a binding's write |
| `event/feed.recovered` | `id`, `binding`, `failures` | when the feed or binding works again; `failures` is how many in a row failed |

Not once per retry: a feed down for an hour says so once.

## Formats

| Format | Chosen by `auto` when | Read into |
|---|---|---|
| `json` | the content type says JSON, or the body starts `{` or `[` | the document as it is |
| `rss` | the content type says XML, RSS or Atom, or the body starts `<` | `{title, link, description, items: [{title, link, summary, published, id, author}]}` for RSS 2.0, RSS 1.0 and Atom alike. HTML is taken out of a summary and a title |
| `csv` | the content type says CSV, or the body has a comma | `{columns: [...], rows: [{column: value}]}`. The first line is the header; a blank header is `column_N`, a repeated one gets `_2` |
| `text` | none of the above | `{text, lines}` |
| `sse` | never; ask for it | each event's `data` lines, read as JSON or as text |

A websocket message is read as JSON when it parses and as `{text, lines}`
when it does not.

## Limits

| What | Limit |
|---|---|
| interval of a polled feed | 5 seconds at least, 30 by default, a day at most |
| one fetch | 10 seconds by default, 1 to 60 |
| a body, a message or an event | 4 MB; more is refused rather than read |
| redirects | five, to `http` or `https` only; never from a network address to this machine |
| writes from a pushed feed | five a second at most; the latest value wins |
| a failing polled feed | tried again after the interval, doubled for each failure in a row, up to five minutes or the interval if longer |
| a pushed feed that drops | opened again after 1, 2, 4 and up to 60 seconds |
| a websocket | pinged every 30 seconds and opened again after 60 with nothing from the server |
| an event stream | opened again after five minutes with nothing, not even a comment |

A body over 256 KB is read on a blocking thread so it does not hold a
runtime worker a client's call is waiting for.

## Where it is kept

`<config>.feeds.json` beside the show's config (`godwinmix.feeds.json` beside
`godwinmix.toml`), written on every change, so a restart brings the feeds
back and a copied show takes them along. Header values are not in it: they
are sealed in the secret store under `feed.<show>.<feed>`, which is per
machine, so a show copied to another machine needs its keys set again with
`feed.set`. A file that will not parse is moved aside to
`godwinmix.feeds.json.unreadable` and the show starts with no feeds, with
an error in the log naming both files.

A core with no config file keeps its feeds for as long as it runs.
