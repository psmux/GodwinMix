# Stream to YouTube, Facebook or Twitch

Send the programme to a platform, and change the stream key later without
opening a config file.

This page takes about five minutes.

## Outputs, not a channel

The programme goes to a platform from the **Outputs** panel. The Channels tab
has platform tiles too, and they do something else: a channel takes a stream
from an encoder pointed at it (OBS, a phone, a hardware encoder) and passes
that stream on. A YouTube tile on a channel nobody is sending to waits, and
YouTube Studio says "No data" for as long as it does.

So the Channels tab says this whenever a channel has platforms and no
encoder. The add form says it above the key box, with **Send the programme
instead** beside **Start sending**: that adds the platform under Outputs with
the server and key you just pasted, and nothing to the channel. A tile already
saved on a waiting channel has **Send the programme to YouTube instead** under
the strip. It asks for the key once more, because a channel keeps its keys
sealed and never hands one back, adds the output, and takes the tile off the
channel unless you switch that off, so two things never publish with the same
key.

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

Press **Start sending** and the form closes on "Connecting to YouTube". That
means the mixer took the address, not that YouTube did: the row under Outputs
says **Connecting** while it dials and **Live** once the platform has accepted
the stream, and a second note says so ("youtube is live"). If it does not get
through, the next section is what you see.

## When it does not connect

The row says what went wrong as soon as the first attempt fails, in a word or
two and then a count (**Refused, trying again (2)**), with a sentence under it
saying what to do. The dot turns red, and an alert pops up once with the same sentence and a **Show
Outputs** button. The header pill says `youtube is not sending` instead of
"Destinations connecting", and hovering it gives the reason. The mixer keeps
trying on its own, backing off, and the programme does not notice any of it.

| The row says | What happened | What to do |
|---|---|---|
| Refused | The server answered and nothing was taking streams on that port | Check the server address and port, and that the server is running |
| No answer | Nothing answered at all | Check the address, and that a firewall or VPN is not blocking outgoing connections to the port (1935 for RTMP, 443 for RTMPS) |
| Server not found | The server name does not exist | Look for a typing mistake in the server address |
| Unreachable | No route from this machine to the server | Check the address and the internet connection |
| Key turned away | The server answered and refused the stream | The stream key is wrong or has expired. Copy it again and paste it under **Edit**, **Replace key** |
| Hung up | The server closed the connection as the stream started | Usually a key the platform does not recognise, or no live stream set up on its side. Check both |
| Stream not taken | The server took the connection and then accepted nothing, until the outage buffer filled (about ten seconds) | The same two causes as Hung up. Some servers turn a wrong key away this way, without an error the sink can read |
| Failed | Anything else | The sentence under the row quotes what the connection said |

The reason stays on the row until the destination is live, then goes. A
destination that was live and drops gets one alert again, as a warning,
saying it lost its connection.

## Replacing a key

A preset writes a destination with `YOUR-STREAM-KEY` where the key goes,
because a preset cannot know yours. The row then says **Needs a stream key**
rather than counting reconnect attempts at you. The mixer never dials an
address that still carries a placeholder (`YOUR-STREAM-KEY`, `your-key`,
`change-me`), so a destination waiting for its key sends nothing to the
platform, has no **Reconnect** button, and the header says destinations need a
key. The example config's YouTube and Facebook destinations start this way.

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

While a destination is not connected the record also has `error`, the reason
from the section above:

```json
{"id":"youtube","state":"reconnecting","reconnects":3,
 "error":{"reason":"refused","message":"127.0.0.1:19351 refused the connection: nothing there is taking streams. Check the server address and port, and that the server is running.",
          "detail":"Connection refused: Could not connect to 127.0.0.1: No connection could be made because the target machine actively refused it."}}
```

`reason` is one of `refused`, `timed-out`, `not-found`, `unreachable`,
`rejected`, `closed`, `stalled` and `other`. `message` is the sentence the page shows.
`detail` is the connection's own words with every part of the address's path
cut out, since a server refusing a stream can quote the stream name back and
the stream name is the key. `event/output.state` carries the same `error`,
and so does the `output.state` hook. Once the destination is live, `error` is
absent.

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

An RTMP, SRT or RIST destination that has been down for 20 seconds with no
error and no retry on the way is rebuilt anyway, and the log says `the output
has been down with no error and no reconnect on the way; rebuilding it`. It is
asked again once every 20 seconds after that, never more often, for as long as
it takes. An SRT listener is the exception: it waits for its callers and is
left alone.

A pulled cable sends no error, so "down" has to be measured. An RTMP
destination is down once no bytes have reached its sink for three seconds,
which happens within a few seconds of the cable going, when the socket's
buffers are full. An SRT destination is down once its receiver has sent no
acknowledgement for six seconds, and a RIST destination once its receiver has
sent no report for six seconds. Pulling the cable between a mixer and a local
RTMP server for 20 seconds and for three minutes, the output read
`reconnecting` within about 8 seconds and was live again within about 2
seconds of the cable going back in, with nobody touching it.

## A rehearsal core will not do it

A core started with `--rehearsal` refuses `output.set` as it refuses
`output.add`, with `-32003` and `data.rehearsal`. Otherwise a rehearsal could
point an existing destination at a real ingest and go on air by the back door.

## See also

* [Receive and send SRT](srt.md)
* [Record to a file](record-to-a-file.md)
* [Send the programme to a WHIP endpoint](send-to-whip.md)
* [The HTTP API](../reference/http-api.md)
