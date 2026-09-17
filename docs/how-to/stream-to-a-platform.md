# Stream to YouTube, Facebook or Twitch

Send the programme to a platform, and change the stream key later without
opening a config file.

This page takes about five minutes.

## From the Outputs panel

Press **Add destination**. Pick the platform. The ingest address is already
filled in, so the only thing to paste is the stream key, and the key goes into
a password box that is never shown again once it is saved.

Where each platform keeps the key:

| Platform | Where to copy it from |
|---|---|
| YouTube | YouTube Studio, Go live, Stream settings. The stream key, not the stream URL |
| Facebook | The Live producer page, Streaming software |
| Twitch | The Creator Dashboard, Settings, Stream. The primary stream key |

**Custom RTMP** is for your own server or a platform that is not on the list:
type the server address and the key separately, or put the whole address in the
server box and leave the key blank. **SRT** takes an `srt://` address and has no
key at all; see [Receive and send SRT](srt.md).

Twitch also publishes ingest servers nearer to you than `live.twitch.tv`. The
default works everywhere and you can paste a closer one over it, but if you do,
paste the key again as well: changing the server rebuilds the whole address and
the mixer will not give the key back to be reused.

## Replacing a key

A preset writes a destination with `YOUR-STREAM-KEY` where the key goes,
because a preset cannot know yours. The row then says **Needs a stream key**
rather than counting reconnect attempts at you.

Press **Add key** on that row, then **Replace key**, paste, and save. The
destination reconnects on its own; the programme and every other destination
are not disturbed.

The same button is **Edit** once a key is in, and the key field there starts
blank with "kept" in it. Saving without touching it leaves the key exactly
where it was, so changing the outage buffer never costs you the key.

## From the API

`output.set` changes one destination in place, naming only what moves:

```sh
curl -X POST http://127.0.0.1:8080/api/v1/outputs/youtube/set \
  -H "Authorization: Bearer $GODWINMIX_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"uri":"rtmp://a.rtmp.youtube.com/live2/abcd-efgh-ijkl-mnop"}'
```

| Field | What it does |
|---|---|
| `uri` | the whole address including the key. Leave it out to keep the one in force |
| `policy` | `own` retries quickly, for a server you run; `cdn` backs off, for a platform |
| `queue_secs` | seconds of encoded video held back, so a short drop is invisible |

The id is not one of them. It is what alerts, hooks and the runtime store call
the destination, so changing it is a `output.remove` and an `output.add`.

The answer is the output record. It does not contain the address, and neither
does anything else: `uri_host` is `rtmp://a.rtmp.youtube.com/…` and the path,
where every CDN puts the key, is never sent anywhere. What you get instead is
`has_key`, which is `false` while the address still carries a placeholder like
`YOUR-STREAM-KEY`. That is enough for a surface to say "needs a stream key" and
put a form up, and not nearly enough to reconstruct the key.

```sh
curl -s -H "Authorization: Bearer $GODWINMIX_TOKEN" \
  http://127.0.0.1:8080/api/v1/outputs/youtube
```

```json
{"id":"youtube","uri_host":"rtmp://a.rtmp.youtube.com/…","has_key":true,
 "state":"live","reconnects":0,"queue_secs":0.4}
```

The change is written to `godwinmix.runtime.toml` beside the config, the same
way `output.add` and `output.remove` are, so it survives a restart.

## What it costs on air

A rebuild of that one destination and nothing else. The encoder is shared and
lives in the programme pipeline, so the other destinations keep their
connections and the programme keeps its frame rate. The outage buffer sits on
the programme side of the swap, which is why several seconds of already encoded
video survive it.

A destination pointed at something that is not answering comes straight back as
`reconnecting` rather than waiting on the connection. The mixer is never held
by one: `/api/status` keeps answering throughout.

## A rehearsal core will not do it

A core started with `--rehearsal` refuses `output.set` as it refuses
`output.add`, with `-32003` and `data.rehearsal`. Otherwise a rehearsal could
point an existing destination at a real ingest and go on air by the back door.

## See also

* [Receive and send SRT](srt.md)
* [Record to a file](record-to-a-file.md)
* [Send the programme to a WHIP endpoint](send-to-whip.md)
* [The HTTP API](../reference/http-api.md)
