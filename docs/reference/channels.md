# Channels

## Destinations

A channel's destinations are where its stream is sent on to as it arrives:
YouTube, Facebook, Twitch, any RTMP or RTMPS server, or an SRT receiver. The
publisher's own bytes are remuxed and sent. Nothing is decoded or encoded, so
a destination costs a socket and a little memory, not a CPU core.

All three methods need the `admin` scope. Each answers the whole channel, with
its `destinations` list as below. A core started with `--rehearsal` refuses
`channel.destination.add` and `channel.destination.set`, as it refuses
`output.add`.

### `channel.destination.add`

`POST /api/v1/channel/destination/add`

| Param | | |
|---|---|---|
| `id` | required | the channel |
| `platform` | required | `youtube`, `facebook`, `twitch`, `custom` or `srt` |
| `label` | optional | what the list calls it. The platform's name when left out |
| `server` | optional | the ingest address. Left out, the platform's own. `custom` and `srt` need one |
| `key` | optional | the stream key. Write only |
| `stream` | optional | which of the channel's streams to send. `*`, the default, is the first live one |
| `enabled` | optional | `true` unless given |

The destination's id is made from its label (`twitch-backup`), or the
platform's id, with `-2`, `-3` on the end when that is taken.

What is refused, with `data.field` naming the field:

* A platform not on the table. `data.platforms` lists the ones there are.
* YouTube, Facebook or Twitch with no key.
* No server for `custom` or `srt`, or a server of the wrong kind: `custom`
  takes `rtmp://` or `rtmps://`, `srt` takes `srt://`.
* A `custom` server with no key and no key on the end of the address.

`srt` keeps no key: an SRT passphrase goes in the address, and the address is
never shown back.

### `channel.destination.set`

`POST /api/v1/channel/destination/set`

`id` and `destination` pick the destination; `label`, `server`, `key`,
`stream` and `enabled` change only what is named. A key left out is kept. An
empty key clears it, which only `custom` allows. Moving a destination to
another platform is a remove and an add.

### `channel.destination.remove`

`POST /api/v1/channel/destination/remove`

`id` and `destination`. The stream to that destination stops and the
destination is forgotten; the publisher and the other destinations are not
touched. It is destructive, so `dry_run: true` answers what it would stop.

### The destination record

| Field | |
|---|---|
| `id` | a slug, unique within the channel |
| `platform` | the platform id |
| `label` | |
| `uri_host` | scheme, host and port, as `rtmps://live-api-s.facebook.com:443`. Never the path or the query, since either can carry a key |
| `has_key` | false while a platform that needs a key has none |
| `stream` | the stream it sends, or `*` |
| `enabled` | |
| `state` | `off`, `waiting`, `connecting`, `live`, `reconnecting` or `failed` |
| `since_ms` | how long it has been in that state |
| `kbps` | what is going out, over the last second |
| `reconnects` | connections lost and made again since it was switched on |
| `error` | the last thing that went wrong, in words, or `null` |

The states:

* `waiting`: on, and nothing to send yet because the stream is not live.
* `connecting`: dialling for the first time. `error` says why the last try
  failed, for instance `nothing answered at rtmp://10.0.0.9:1935`.
* `live`: sending.
* `reconnecting`: the far end went away and is being dialled again. A server
  you run is retried from 100 ms up to every 2 s; YouTube, Facebook and
  Twitch from 1 s up to every 30 s, the same two policies as outputs.
* `failed`: the far end refused the key, as in `YouTube refused the key
  (...)`. It is asked again after the longest wait, and after three refusals
  in a row it stops until the destination is changed.

On every connection the far end gets the stream's metadata and codec headers
first, then nothing until a keyframe, so a platform never sees a picture it
cannot decode. A destination that falls behind loses whole GOPs from the
front of its own queue and starts again at the next keyframe; the publisher
and the other destinations do not wait for it.

The platform servers are in `godwinmix_protocol::destination::PLATFORMS`.
The web page's form keeps its own copy in `ui/client/destinations.js`, and a
test fails when the two disagree.
