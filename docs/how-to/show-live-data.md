# Show live data on air

Headlines from a news feed in the ticker, the score from a sports API in the
score bug, the guest list from a spreadsheet in the lower thirds. Point the
mixer at the feed once, bind a value from it to a source, and the mixer keeps
it current: it fetches on its own, and writes to the source only when the
value has changed. A text changes in place, with no rebuild and no gap.

The feed is fetched by the mixer, off the video path, one small task per
feed. A show with no feeds runs nothing for them at all.

The examples below use the three sample feeds in `examples/live-data/`, so
every command works on a machine with no internet. Serve them from a
terminal in the repository:

```sh
python3 -m http.server 8765 --bind 127.0.0.1 --directory examples/live-data
```

Calls go to the control port with `curl`. Add an `Authorization: Bearer
<token>` header if your mixer has tokens. Every call here is also an MCP tool
(`test_feed`, `add_feed`, `bind_feed`, `list_feeds` and the rest, behind
`search_tools`), and the page has a dialog for all of it, at the end.

## Headlines into a ticker

Make the ticker:

```sh
gmx ctl source add crawl "ticker:Waiting for news"
```

Look at the feed before keeping it. `feed.test` fetches once, stores nothing,
and says what is in it: the format it was read as, the top keys, and every
path with an example.

```sh
curl -s localhost:8080/api/v1/feed/test -H 'content-type: application/json' \
  -d '{"address": "http://localhost:8765/news.rss"}' | jq -c '.format, .keys, .paths[]'
```

```
"rss"
["description","items","link","title"]
{"example":"What is happening in town tonight.","path":"description"}
{"example":"a list of 3","path":"items"}
{"example":"","path":"items[].author"}
{"example":"","path":"items[].id"}
{"example":"https://news.example/polls","path":"items[].link"}
{"example":"Fri, 02 Oct 2026 19:00:00 GMT","path":"items[].published"}
{"example":"Every ward reports by midnight.","path":"items[].summary"}
{"example":"Polls close at ten & the count starts at once","path":"items[].title"}
{"example":"https://news.example/","path":"link"}
{"example":"Town News","path":"title"}
```

RSS and Atom both arrive as `items[]` with the same keys, and a summary has
its HTML taken out. `items[].title` is every headline. Try it with a limit:

```sh
curl -s localhost:8080/api/v1/feed/test -H 'content-type: application/json' \
  -d '{"address": "http://localhost:8765/news.rss", "select": "items[].title", "limit": 2}' | jq -c .value
```

```
["Polls close at ten & the count starts at once","Rain later, clearing by morning"]
```

`value` is exactly what a binding would write. Keep the feed, fetched once a
minute, and bind the first ten headlines to the ticker's items:

```sh
curl -s localhost:8080/api/v1/feed/add -H 'content-type: application/json' \
  -d '{"id": "news", "address": "http://localhost:8765/news.rss", "interval_s": 60}' | jq -c '{id, kind, state}'
```

```
{"id":"news","kind":"polled","state":"starting"}
```

```sh
curl -s localhost:8080/api/v1/feed/binding/add -H 'content-type: application/json' \
  -d '{"feed": "news", "select": "items[].title", "limit": 10, "to": {"source": "crawl", "path": "params.items"}}' | jq -c '{id, writes, value}'
```

```
{"id":"crawl-items","writes":1,"value":["Polls close at ten & the count starts at once","Rain later, clearing by morning","Bridge reopens to traffic"]}
```

The binding writes at once when the feed has been read, and after that only
when the headlines change. The id was made from the target; give `id` to name
it yourself.

An unchanged feed costs one request and nothing else. The sample server
sends `Last-Modified`, the mixer sends it back, and the server answers `304
Not Modified` with no body. After a few minutes:

```sh
curl -s localhost:8080/api/v1/feed/list | jq -c '.feeds[] | {id, state, fetches, not_modified}'
```

```
{"id":"news","state":"ok","fetches":4,"not_modified":3}
```

and the binding has still written once. A server that sends an `ETag` is
treated the same way. One that sends neither is fetched whole, and if the
body is the same as last time it is not even read.

## A score into a score bug

A text source is a score bug. Make one and look at the score feed:

```sh
gmx ctl source add score "text:0 : 0"
curl -s localhost:8080/api/v1/feed/test -H 'content-type: application/json' \
  -d '{"address": "http://localhost:8765/score.json", "select": "match", "template": "{home} {home_score} : {away_score} {away}"}' | jq -c .value
```

```
"Leeds 2 : 0 Hull"
```

The template fills each `{path}` from what `select` picked. A path that
picks nothing is refused with what was there, so the next try is a read
rather than a guess:

```sh
curl -s localhost:8080/api/v1/feed/test -H 'content-type: application/json' \
  -d '{"address": "http://localhost:8765/score.json", "select": "match.home_goals"}' | jq -r .error.message
```

```
`match.home_goals` selects nothing: `match` has no key `home_goals`. It has: away, away_score, home, home_score, minute. The top of the document has: match. Pick a path from data.paths, or call feed.test with no select to see the whole document.
```

Keep it, every ten seconds, and bind it:

```sh
curl -s localhost:8080/api/v1/feed/add -H 'content-type: application/json' \
  -d '{"id": "scores", "address": "http://localhost:8765/score.json", "interval_s": 10}'
curl -s localhost:8080/api/v1/feed/binding/add -H 'content-type: application/json' \
  -d '{"feed": "scores", "select": "match", "template": "{home} {home_score} : {away_score} {away}", "to": {"source": "score", "path": "params.text"}}' | jq -c '{id, writes, value}'
```

```
{"id":"score-text","writes":1,"value":"Leeds 2 : 0 Hull"}
```

Change `home_score` to 3 in `examples/live-data/score.json`. Within ten
seconds the text on air reads `Leeds 3 : 0 Hull`, and the binding has
written twice. Change it back and it writes a third time.

The shortest interval is 5 seconds, so no server is asked too often. A real
sports API usually wants a key: put it in `headers`, never in the address.

```json
{"id": "scores", "address": "https://api.example/match/123", "interval_s": 15, "headers": {"x-api-key": "..."}}
```

The value is sealed in the mixer's secret store as it arrives and never
comes back: `feed.list` shows `"x-api-key": "__secret__"`, and sending
`"__secret__"` back in `feed.set` keeps the stored one. It is never written to
a log.

For a score that changes every few seconds, a feed that pushes is better
than one that is polled. A `wss://` address is read as a websocket, each
message one JSON document; an `https://` address with `"format": "sse"` is
read as Server-Sent Events. Either is written at most five times a second
however fast it sends.

## A published sheet into lower thirds

In Google Sheets, File, Share, Publish to web, pick the sheet and
"Comma-separated values". The address it gives is a CSV feed. The sample
stands in for one here.

```sh
gmx ctl source add strap "text:Name"
curl -s localhost:8080/api/v1/feed/test -H 'content-type: application/json' \
  -d '{"address": "http://localhost:8765/guests.csv"}' | jq -c '.format, .keys, .paths[]'
```

```
"csv"
["columns","rows"]
{"example":["Name","Title"],"path":"columns"}
{"example":"a list of 3","path":"rows"}
{"example":"Ada Lovelace","path":"rows[].Name"}
{"example":"Analyst","path":"rows[].Title"}
```

Each row is keyed by the header row. Bind the first guest to the strap, a
name over a title:

```sh
curl -s localhost:8080/api/v1/feed/add -H 'content-type: application/json' \
  -d '{"id": "guests", "address": "http://localhost:8765/guests.csv", "interval_s": 30}'
curl -s localhost:8080/api/v1/feed/binding/add -H 'content-type: application/json' \
  -d '{"id": "guest", "feed": "guests", "select": "rows[0]", "template": "{Name}\n{Title}", "to": {"source": "strap", "path": "params.text"}}' | jq -c '{id, writes, value}'
```

```
{"id":"guest","writes":1,"value":"Ada Lovelace\nAnalyst"}
```

When the next guest sits down, point the binding at their row:

```sh
curl -s localhost:8080/api/v1/feed/binding/set -H 'content-type: application/json' \
  -d '{"id": "guest", "select": "rows[2]"}' | jq -c '{id, writes, value}'
```

```
{"id":"guest","writes":2,"value":"Johnson, Katherine\nMathematician"}
```

A producer who edits the sheet changes the strap within the interval, and
nobody touches the mixer.

## Other targets

`to` takes three shapes:

* `{"source": "<id>", "path": "params.<name>"}`: any param a source takes,
  through `source.set`. `params.text` for a text, `params.items` for a
  ticker. A nested path, `params.fields.headline`, changes one field and
  keeps the others beside it.
* `{"graphic": "ograf/lower-third", "field": "name"}`: a field of an OGraf
  graphic on a scene, through `scene.apply_graphic` with no play, which is
  the graphic's update action. Add `"item"` when the graphic is on the canvas
  twice. See [Make a graphic](make-a-graphic.md).
* `{"scene_param": "speaker"}`: a scene parameter, through
  `scene.params.set`, so every `{{speaker}}` on every scene follows the feed.

## From the page

Sources, Live data opens the dialog. Paste an address and press Try: what
came back is listed as paths with an example each. Press a path to pick it,
add a template or a limit and press Preview to see what would be written,
choose the source and the param, and press Bind. A new feed is added for you,
named after the address's host.

The list at the top shows every feed with a dot: green and when it was last
read, red with the reason it is failing, grey when paused. Under each feed
are its bindings and what each last wrote, with Hold and Unbind. The list is
read again every two seconds while the dialog is open, and not at all once it
is closed.

The editor of a text or a ticker (the gear on its tile) has Fill from live
data, which opens the same dialog with that source already picked.

## When something is wrong

* **A feed says failing.** `last_error` says why and what to do: a refused
  connection, a timeout, `401` (check the key in the headers), `404` (check
  the address in a browser), a body that would not read. It is tried again
  after the interval, doubled for each failure in a row, up to five minutes.
  `event/feed.failed` is sent once when it starts failing and
  `event/feed.recovered` once when it works again. What it last wrote stays on
  air the whole time.
* **A binding has `last_error`.** The value could not be written, usually
  because the source was removed. It is tried again on every fetch, changed
  or not, so when the source is back the value lands without waiting for the
  feed to change.
* **A feed over 4 MB** is refused rather than read. Ask the server for fewer
  items.
* **`file:` and other addresses** are refused: a feed is fetched over the
  network only, from `http`, `https`, `ws` or `wss`.

## What it costs

Measured on an Apple M series laptop with a release build: a show with five
text sources and five feeds each polled every 5 seconds (an RSS feed and a
JSON feed answering `304`, a CSV and a 1 MB JSON with no validators fetched
whole, a JSON score changing every 30 seconds) used 2.29 seconds of CPU a
minute, against 2.10 with the same feeds paused: about 0.3 percent of one
core for the five. Resident memory with three unchanged feeds polling stayed
flat at 159 to 163 MB over three minutes, the same as with them paused. A
show with no feeds starts no task, builds no HTTP client and holds nothing
but an empty list.

See [the live data reference](../reference/live-data.md) for every method
and event, and [feed paths and templates](../reference/feed-paths.md) for
what `select` and `template` take.
