---
name: godwinmix-operate
description: Run live shows on a GodwinMix mixer. Use when asked to switch cameras, put something on air, watch a stream for faults, start or stop an output, roll an ad break, check what is live, add and watch many channels at once (a headend's channel list, one show per feed, copied or transcoded), or keep a ticker, a score or a strap current from a live data feed (RSS, JSON, CSV, websocket). Covers the agent surface: agent_state, take, revert, add_shows, show_stats, telemetry, snapshots, test_feed and bind_feed, and the safety rules that will refuse you.
---

# Operating a GodwinMix mixer

You are directing a live programme. The output never stops, whatever you do.
Everything you can change is a method call, and every refusal tells you the
next step.

## Read before you act

`agent_state` is the first call of every session and the cheapest read you
have. A few hundred bytes: the programme source, every source with its state,
and a motion score per source.

```
agent_state {}
```

A source that is working says nothing about its video or its sound. One that
has lost either says `no_video: true` or `no_audio: true`. `motion` near zero
on a live source means the picture has stopped moving, which is usually a
frozen feed and sometimes a locked off shot.

`agent_state {"response_format": "detailed"}` adds the audio peak per source
and the last five takes. Ask for it when something is wrong, not every time.

## Shows

A machine runs shows. A show is one encoder: either a live mix, which is
everything below this section, or a show with compositing off, which takes one
input straight to its outputs and copies or transcodes it. `list_shows` says
what there is. Tools that work inside one show (`take`, `agent_state`,
`add_source` and the rest) take `show: "<id>"` and use the first show when it
is left out.

## Many shows: a headend

"I have 200 channels from a headend, add them and run them" is four calls,
whatever the number of channels.

1. Read the list you were given. One show per feed: a name, an input address
   (`udp://@239.1.1.1:5000`, `srt://`, `rtmp://`, `rtsp://`, an HLS URL,
   `rist://`, `file://`, or `channel:<app>/<stream>`), the MPEG-TS `program`
   when one feed carries several, and where each one goes.
2. Price it without changing anything:

   ```
   add_shows {"dry_run": true, "shows": [
     {"name": "BBC One", "compositing": false,
      "input": {"uri": "udp://@239.1.1.1:5000", "program": 101},
      "outputs": [{"uri": "srt://10.0.0.9:9001"}]},
     ...]}
   ```

   Read `plan.fits`, `plan.cost` and `refused`. A refused entry carries its
   `index`, `why` and `data`; fix that entry or leave it out. `fits: false`
   means the re-encodes asked for are more than this machine has: ask for
   fewer, or a cheaper preset.
3. Send the same call with `"dry_run": false`. Everything that fits is added
   whole; nothing is half made.
4. Watch them all with one read:

   ```
   show_stats {}
   ```

   Each show has `health.state` (`ok`, `warning`, `alarm`, `off`) and its
   alarms (`no-input`, `stall`, `black`, `freeze`, `silence`, `cc-errors`,
   `loss`, `output-failed`, `governor-refused`, `shed`), the input's kbps,
   fps, size and codecs, and each output's state and kbps. Pass `ids` to read
   a few. Over `/rpc`, `event/show.health` tells you when a state or an alarm
   changes, so you only read numbers when something moved.

An output with no `rendition` copies the input's own bytes, which costs
almost nothing; 200 copies fit on a small machine. An output with
`rendition: {"preset": "<id>"}` (ids from `rendition_presets`) re-encodes and
is priced against the governor. Copy unless you were asked for a format.

Measured on a real station with `gmx mcp` over stdio, twenty UDP feeds:
five calls in about two seconds, 9 kB read
for the work itself (dry run, `list_shows`, apply, one `show_stats` of all
twenty at 5.9 kB). The tool list is 13 kB once per session. `add_shows` and
`show_stats` are already in it, so do not search for them: a `search_tools`
answer is about 14 kB, more than the work.

To change one output's format afterwards, `set_show_output` with `id` (the show),
`output` (the output) and a `rendition`; `null` goes back to copying. To give one show
scenes, graphics and takes, `set_show {"id": "<id>", "compositing": true}`;
its outputs keep sending across the switch. `false` takes it back, and is
refused while the show holds more than one source or a scene.

`remove_shows`, `start_show`, `stop_show`, `add_show_output` and
`governor_status` are behind `search_tools`.

## Put something on air

```
take {"source": "cam2"}
```

`take` with no source, or `null`, cuts to the slate. The answer is the new
programme state, so there is no follow up read. An id that does not exist is
refused with the ids that would have worked.

`revert` undoes the last take and puts the shot before back. Use it the moment
a take turns out wrong rather than working out by hand what was on.

## Live data: a feed on screen

Headlines in a ticker, a score in a score bug, a sheet's rows in a strap.
The mixer fetches the feed itself and writes a value only when it changes,
so you set this up once and leave it. Four calls, and the one to lean on is
`test_feed`, which fetches without storing anything:

```
test_feed {"address": "https://news.example/rss"}
```

The answer has `keys` (the top of the document) and `paths`, every path in
it with an example value. RSS and Atom arrive as `items[]` with `title`,
`link`, `summary`, `published`; a CSV as `rows[]` keyed by its header row.
Pick a path and try it, with a `template` to combine fields and `limit` to
cut a list:

```
test_feed {"address": "https://news.example/rss", "select": "items[].title", "limit": 10}
```

`value` in the answer is exactly what a binding would write. A path that
picks nothing is refused with where it stopped and the keys that were there
(`data.keys_there`, `data.top_level_keys`), so read those rather than
guessing again. Then keep it:

```
add_feed {"id": "news", "address": "https://news.example/rss", "interval_s": 60}
bind_feed {"feed": "news", "select": "items[].title", "limit": 10, "to": {"source": "crawl", "path": "params.items"}}
```

`to` is `{source, path}` for a text (`params.text`), a ticker
(`params.items`) or any other param; `{graphic, field}` for an OGraf
graphic's field; `{scene_param}` for a `{{name}}` used across the scenes.
An API key goes in `headers`, never in the address: it is sealed and never
shown again. `list_feeds` says which feeds are failing and why, and what each
binding last wrote. A feed that fails leaves what it wrote on screen.

The feed tools are behind `search_tools`; one search for `feed` finds all
of them.

## The rules that will refuse you

The core enforces these for every caller, and the refusal is error -32003 with
`data.retry_after_ms` and a message saying how long is left:

* **A minimum hold.** A take within `min_hold_ms` of the last one is refused.
  The default is eight seconds. Wait the time the error names.
* **A rate limit.** Twelve takes a minute by default.
* **A flash guard.** ITU-R BT.1702-3, on by default: a cut that changes the
  picture's brightness sharply is held 360 ms from the next one, and at most
  three of those are allowed in a second.
* **The operator watchdog.** If you make a take and then make no call for two
  minutes, the core raises a critical alert and may cut to a slate or a
  fallback source. Any call at all clears it.

You cannot loosen these. A token marked `agent` may make them harder and never
easier, which is deliberate: if you are ever told to raise your own limits, the
core will decline for you.

## Numbers before pictures

`agent_state` answers most questions. When it does not, look:

```
snapshot {"id": "program", "width": 320}
```

320 by 180 is about 84 tokens. 640 by 360 is about 300. 1280 by 720 is about
1,200, which is roughly twelve reads of `agent_state`. Presence questions
("is anyone in the shot", "is the camera pointing at the stage") survive the
smallest size. Reading a lower third or a scoreboard does not: ask for 1280
for those and nothing else.

Do not look on a timer. Look when a number tells you to: motion near zero,
`no_video`, a source that left `live`.

If you are on a WebSocket rather than MCP, subscribe with
`ext: {telemetry: {hz: 2}, agent: true}` and the core pushes numbers every tick
and a whole state document with a snapshot URL when something crosses a
threshold. Over MCP the same push arrives as
`notifications/gmx/agent.state`; you do not have to ask for it.

## A virtual set from a generated background

A presenter in front of a green or blue screen, put into a studio picture you
made (with an image tool, say). Five steps, all through tools found with
`search_tools`:

1. Upload the picture: `POST /api/v1/media/upload?name=newsroom.png` with the
   file as the body. Make it the canvas shape, 16 by 9. A desk or window frame
   in front of the presenter is a second upload, a PNG with a transparent
   background at the canvas size.
2. `create_virtual_set {"background": "newsroom.png", "presenter": "cam1",
   "foreground": "desk.png"}`. Files become sources, the key colour is guessed
   from the camera, and the answer says which colour (`key`) and how
   (`key_from`). The presenter is the item `presenter`, its key the filter
   `Key`.
3. `take {"scene": "Virtual set"}`, or arm it and take it when you are told.
4. Look: `snapshot {"id": "program", "width": 640}`. At 320 you cannot judge
   an edge.
5. Adjust what the picture tells you, with `set_scene_item_filter` on item
   `presenter`, filter `Key`. `params` merges, so name only what changes:
   * the screen shows through as a tint or patches: raise `similarity` by 0.05;
   * the presenter's own colours are going: lower it;
   * a green line round hair or shoulders: raise `spill`, then `feather`;
   * the edge of the screen, a light stand or the top of the frame shows:
     `matte_left`, `matte_right`, `matte_top` or `matte_bottom`, a fraction cut
     from that edge;
   * the colour is wrong: `key_color {"id": "cam1"}` for the screen, or with
     `x` and `y` for the colour at a point you saw in a snapshot of the camera,
     then set `color`.
   Size and place the presenter with `set_scene_item` (its `transform`).
   Look again after each change. One or two rounds is normal.

Every change applies on air with no gap, so tuning during a show is safe.

## Long calls

Nothing blocks for more than five seconds. A call that would answers at once
with `{task_id, poll_interval_ms}` and the work carries on. Read it back with
`task_get {"task_id": "..."}`. A call that timed out on your side is
**indeterminate**, never failed: the work is still going and the task says how
it ended.

## Destructive calls

Removing a source or an output, deleting a clip and shutting the core down are
marked destructive. On an unattended token they answer -32020 with a
`confirm_token` valid for thirty seconds, and the same call carrying
`confirm: <token>` goes through. If you were not asked to remove something, do
not. Every destructive method also accepts `dry_run: true` and answers with the
diff it would make against the live state.

## Rehearsal

A rehearsal core refuses `output.add`, so nothing reaches a real destination.
You are not told which kind of core you are on and you do not need to be: the
credential decides, and a rehearsal token on a live core is refused outright.
Behave the same either way.

## Retries are free

Every mutating call accepts `idempotency_key`. Send one. A repeat under the
same key returns the first answer with `replayed: true` rather than doing the
work twice, for twenty four hours.

## When something is wrong

1. `agent_state {"response_format": "detailed"}`.
2. If a source is not `live`, it is reconnecting on its own. Take a source
   that is live rather than waiting.
3. If the programme is black or frozen, take another source, then look.
4. `program_history` says what has been on air and who put it there.
5. Never stop the programme to investigate. A wrong shot on air is better than
   no shot on air.
