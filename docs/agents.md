# Directing the programme from an AI agent

The mixer is a daemon with an HTTP API, and nothing about that API assumes a
human is on the other end. This page is for the case where the operator is a
language model: something that reads the state of the sources, looks at a
picture when the numbers are not enough, and decides which source is on
programme. It covers the three endpoints built for that, the decision loop,
the MCP server for Claude Code and other clients, the token, the go live call
a customer's own backend makes, what to expect in latency, and the short list
of things an agent must not do.

`examples/ai-director.py` is a working director built on this page. Read it
alongside.

## What the mixer gives an agent

Three things, all on the same port as everything else.

| | |
|---|---|
| `GET /api/agent/state` | one compact JSON document, sized for a model's context |
| `GET /api/snapshot/sheet.jpg` | every source and the programme in one mosaic image |
| `POST /api/take` | `{"source": "cam1"}`, the same call the UI makes |

`/api/status` still exists and is the full picture. `/api/agent/state` is the
part of it a director needs, in the order a director needs it:

```json
{
  "program": "cam1",
  "uptime_secs": 5412,
  "sources": [
    {"id": "cam1", "name": "Stage", "state": "live", "superimposed": false,
     "has_video": true, "has_audio": true, "video_idle_ms": 0, "motion": 0.31},
    {"id": "score", "name": "Scoreboard", "state": "live", "superimposed": true,
     "has_video": true, "has_audio": false, "video_idle_ms": 0, "motion": 0.0},
    {"id": "guest", "name": "Guest", "state": "connecting", "superimposed": false,
     "has_video": false, "has_audio": false, "video_idle_ms": 0, "motion": 0.0}
  ],
  "outputs": [{"id": "youtube", "state": "live", "reconnects": 1}],
  "program_motion": 0.31,
  "backend": "nvidia",
  "snapshots": {
    "sheet": "/api/snapshot/sheet.jpg",
    "program": "/api/snapshot/program.jpg",
    "source": "/api/snapshot/{source_id}.jpg"
  }
}
```

`program` is the id on air, or `null` when the programme is showing the slate.
A source's `state` is one of `connecting`, `live`, `stalled` or `failed`, and
only `live` may be taken. `video_idle_ms` is how long since that source last
produced a frame. `motion` is a number from 0.0 to 1.0 saying how much the
source's picture changed between its last two frames: a static slide is near
0, a talking head is around 0.1 to 0.3, a sports feed with the camera panning
goes higher. `program_motion` is the same number for what is on air.

The snapshots are JPEGs. `sheet.jpg` is a mosaic with every source and the
programme, each cell labelled with its id, and is what an agent should look at
when it needs to see anything: one image, one request, and the model can
compare sources side by side. `program.jpg` and `{source_id}.jpg` are single
pictures for when the question is about one of them. All three take
`?width=N` to scale down. 1280 is plenty for a model to read a scoreboard from
the sheet; smaller is cheaper and usually still enough.

## The decision loop

An agent that directs a programme runs the same loop a human operator runs
without thinking about it.

1. Read `/api/agent/state`.
2. Decide whether it needs to see the picture. Most of the time it does not
   (next section). When it does, fetch `/api/snapshot/sheet.jpg`.
3. Decide which source should be on programme, given the goal it was told.
4. If that differs from `program`, and the source is `live`, `POST /api/take`.
5. Wait 1 to 3 seconds and go round again.

A take is a property change on a compositor pad and lands on the next frame, so
the loop does not need to anticipate. Polling every second is fine. Polling
faster than the frame rate buys nothing, because nothing in the state changes
between frames.

The interval also sets the tempo of the programme. A director that changes its
mind every second produces something nobody wants to watch, so hold a shot for
a minimum time after a take (the example uses eight seconds) regardless of what
the model says, unless the source on air has stopped.

### Reading the numbers instead of the picture

Sending an image to a model costs more and takes longer than sending a few
hundred bytes of JSON, and the JSON answers the common questions by itself:

* Is anyone talking on this source? `has_audio` says whether it carries sound
  at all. The mixer does not report a level, so a picture of the source, or
  knowing which source is the presenter, does the rest.
* Has the source frozen? `video_idle_ms` climbs while `state` is still `live`.
  A camera that has been idle for a couple of seconds is about to be `stalled`.
* Has the scoreboard changed? A scoreboard page has `motion` near zero until
  the score changes, then a spike. Watch for the spike and look at the sheet
  then, not every cycle.
* Has the shot gone dead? `program_motion` sitting at zero for several cycles
  on a source that should be moving is a reason to look.

The example fetches the sheet on a slow timer (every fifth cycle by default)
and immediately when any source's `motion` moves by more than a threshold from
the previous reading. In practice that means the model sees a picture a few
times a minute rather than every second, and the numbers carry the rest.

### Deciding

Give the model the goal in plain words, the state JSON, and the sheet when you
have one, and ask for a small JSON answer: `{"take": "score", "reason": "score
changed to 2-1"}`, or `{"take": null, ...}` to leave the programme alone. Do
not let the model call the API directly from a free text answer. Parse the
decision, check the source exists and is `live`, check the minimum hold time,
then post the take yourself. The example does exactly this and refuses
anything that fails the checks, with the reason in its log.

A model will sometimes return prose around the JSON, or no JSON at all. Treat
that as "no change" and carry on. It is a live programme; a missed decision is
recoverable and a bad take is on air.

## Adding a source from a URL

Sources are added with `POST /api/sources` and the protocol is worked out from
the address, so an agent that is told "put the scoreboard at this URL on air"
needs one call:

```sh
curl -X POST http://HOST:8080/api/sources \
  -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"uri":"https://example.com/scoreboard","kind":"web","name":"Scoreboard","superimpose":"auto"}'
```

`kind: "web"` renders the page in a real Chromium, with its audio. An
`rtmp://`, `rtsp://`, `srt://` or `.m3u8` address is opened as a stream and
`kind` can be left out. `id` can be left out too and is derived from the name.

`superimpose` is a per source choice for web pages, and the rule for an agent
is simple. Use `"auto"` when the page is mostly a video player: a match feed,
a stream embed, a page whose point is the video on it. The mixer then decodes
that video itself on the GPU and the browser only draws the page's chrome over
it, which saves about a CPU core per source and makes the picture sharper.
Use `"off"` when the page is the content: a scoreboard, a slide deck, a
dashboard, a page with several small clips whose player controls should be
visible. `auto` falls back to full rendering on its own when the page's video
cannot be handed over (YouTube and every other MSE player, and anything with
DRM), so choosing `auto` is never wrong, it is only sometimes slower to come
up. `superimposed` in the state says which happened.

A newly added source is `connecting` until its first frame arrives. Do not take
it until it reads `live`. Poll the state; it will change within the times in
"Latency" below.

Remove a source with `DELETE /api/sources/{id}`, and never the one that is on
programme without taking another first.

## The token

Anyone who can reach the control port can switch the programme, so put the
port behind a token on any network you do not own entirely:

```toml
[control]
bind = "0.0.0.0:8080"
token = "a-long-random-string"
```

`LIVEBOXMIX_TOKEN` in the mixer's environment does the same and suits a
container. With a token set, every request carries it:

```
Authorization: Bearer a-long-random-string
```

`GET` requests and the WebSocket also accept `?token=...` in the URL, which is
how a browser `<img src="/api/snapshot/sheet.jpg?token=...">` or a plain
`curl` can fetch a picture. The POSTs take the header only.

Without a token configured the port is open, which is right for a mixer on a
laptop and wrong for one in a data centre.

## MCP: giving Claude Code the mixer as tools

`liveboxmix mcp` is an MCP server over stdio. It is a client of the same HTTP
API and exposes it as tools, so a model in Claude Code, Claude Desktop or any
other MCP client can direct the mixer by name rather than by assembling
requests.

```sh
claude mcp add liveboxmix -- liveboxmix mcp --url http://HOST:8080 --token TOKEN
```

Drop `--token` when the mixer has none. The binary can run on the workstation
and point at a mixer elsewhere; nothing about it needs to be on the same
machine as GStreamer.

For other clients, the equivalent configuration is the usual stdio server
entry:

```json
{
  "mcpServers": {
    "liveboxmix": {
      "command": "liveboxmix",
      "args": ["mcp", "--url", "http://HOST:8080", "--token", "TOKEN"]
    }
  }
}
```

The tools, and the API call behind each:

| tool | does |
|---|---|
| `status` | `GET /api/status`, the full state |
| `agent_state` | `GET /api/agent/state`, the compact one |
| `take` | `POST /api/take`; `source` null cuts to black |
| `add_source`, `remove_source` | `POST /api/sources`, `DELETE /api/sources/{id}` |
| `list_outputs`, `add_output`, `remove_output`, `reconnect_output` | the `/api/outputs` calls |
| `ad_break`, `end_ad_break` | `POST /api/adbreak`, `POST /api/adbreak/end` |
| `snapshot` | a snapshot JPEG, returned as an image the model can look at; the sheet, the programme, or one source |
| `go_live` | `POST /api/golive`, below |

A session in Claude Code then goes: "look at the sheet and tell me which
source has the presenter", "take it", "add https://... as a source called
Scoreboard and superimpose it", "when the score changes cut to it for ten
seconds and come back". The model reads the state, looks when it must, and
calls `take`. Everything on this page about hold times and not taking a
source that is not live applies to a model driving the tools directly, and is
worth saying to it in the conversation or in a project's instructions.

## Go live from a customer's button

The typical product built on the mixer has a customer facing page with a
"Go Live" button. The page does not talk to the mixer. The customer's backend
does, with the token, once it has decided the customer is allowed to:

```sh
curl -X POST http://HOST:8080/api/golive \
  -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"url":"https://example.com/event/42","rtmp":"rtmp://a.rtmp.youtube.com/live2/KEY","superimpose":"auto"}'
```

```json
{"source":"event-42","output":"out-1","state":"connecting"}
```

The reply is `202` straight away. The mixer adds the page as a web source, adds
the RTMP destination if one was given (leave `rtmp` out to stream to the
outputs already configured), and takes the source to programme by itself once
it is `live`. `id` fixes the source id when the backend wants to choose it. The
backend polls `/api/agent/state` to show the customer "connecting" turning
into "live", or just trusts the mixer, which will have done the take within
the times below.

The browser never holds the token and never reaches the control port. That is
the whole reason the call is shaped for a backend and not for a page.

## Latency

| | |
|---|---|
| a take | the next frame, 33 ms at 30 fps; the request returns before the cut lands |
| a scheduled take (`at_running_time_ms`) | armed on the pipeline clock, the same mechanism as a scheduled ad cue, which measured +4 ms against the requested time |
| a new stream source (RTMP, HLS, SRT) | `live` in 2 to 10 seconds, longer if the client has to be switched (see the README on `rtmp_client`) |
| a new web source | `live` in 5 to 20 seconds: Chromium starts, the page loads, the first frame is painted |
| a superimposed web source | add a few seconds for the probe that asks the page what it plays; 1 to 6 seconds when a video is found, up to 20 when there is nothing to find |
| a go live call | the sum of the above, then the take |
| an output reconnect | about 2 seconds |
| a state read or a snapshot | milliseconds; the snapshot is encoded on request from the frame the mixer already holds |

These are the figures measured while the features were built and on the test
rig described in the README. A page that is slow to load is slow here too.

## Things an agent must not do

* Take a source whose state is not `live`. The API accepts the take, because
  a human operator sometimes wants to cut to a source the instant it comes up,
  and the programme shows the slate until it does. An agent has no such
  reason. Check the state first.
* Poll faster than the frame rate. Nothing changes between frames. Once a
  second is as fast as there is any reason to go; every 2 to 3 seconds is
  enough for most programmes.
* Remove the source that is on programme without taking another first. The
  programme drops to the slate. Take, then remove.
* Fetch the sheet on every cycle when the numbers say nothing has changed.
  It works, it is just slow and expensive for no gain.
* Change shot on every cycle. Hold a shot for a minimum time. The example
  enforces this outside the model, and so should anything built from it.
* Act on a model answer without parsing it. Extract the JSON, check the id
  against the source list, check the state, then call the API. A model that
  answers in prose gets a "no change", not a guess.
* Send `POST /api/shutdown` as part of directing. It stops the mixer. It is
  not a "stop streaming" call; that is `DELETE /api/outputs/{id}`.

## The example

`examples/ai-director.py` runs the loop above against a real mixer and a
Claude model. It is one file, Python 3, needs the `anthropic` package and
nothing else outside the standard library.

```sh
pip install anthropic
export ANTHROPIC_API_KEY=...
python3 examples/ai-director.py --url http://HOST:8080 --token TOKEN \
  "Show the source with a person speaking. Cut to the scoreboard when the score changes, hold it ten seconds, then go back."
```

`--dry-run` logs the decisions without posting any take. `--interval` sets
the cycle in seconds (default 2). `--look-every` sets how many cycles pass
between pictures when nothing moves, and `--motion-delta` sets how much a
source's `motion` must change to trigger a look early. `ANTHROPIC_MODEL`
picks the model; the default is `claude-sonnet-5`.

A rule based director with no model in it, which follows an external schedule
instead, is at `deploy/isp/director/director.py`. It is the other shape a
director takes: when the decision is a lookup, no model is needed.
