# Wave 2: renditions reach real outputs

The shapes four agents build against at the same time. The design is
dev/plans/shows-and-renditions.md; the shared types are in
crates/godwinmix-protocol/src/rendition/. Change this file first, in its own
commit, if a shape has to move.

## Who owns what

| Agent | Owns | Delivers |
|---|---|---|
| graph | crates/godwinmix-core (new module `render/`), crates/godwinmix-govern glue, output methods | the programme's outputs planned and built from renditions, the governor wired in |
| hls | crates/godwinmix-core (new module `hls/`), crates/godwinmix/src/control (the `/hls/` routes) | HLS, LL-HLS and DASH out, ABR ladders, served on the control port |
| chanxcode | plugins/ingest (destinations), crates/godwinmix/src/channels | a channel destination that asks for a rendition is transcoded, planned and admitted like everything else |
| rendui | ui/ | choosing renditions and presets, seeing what the plan did and why, refusals with advice |

## Protocol

An output (programme output or channel destination) carries an optional
rendition request. Absent means what it means today: the programme's own
encode, or a copy of the channel stream.

    output.add / output.set      {..., rendition?: RenditionRequest | {preset: "<id>"} | {ladder: [RenditionRequest]}}
    channel.destination.add/set  {..., rendition?: RenditionRequest | {preset: "<id>"} | {ladder: [RenditionRequest]}}

`output.set` with `rendition: null` clears the format: the output goes back
to the programme's own encode (a channel destination back to a copy).
Leaving `rendition` out keeps what it has. `{ladder: [...]}` is a custom ABR
ladder, top rung first, for an `hls/output`; each rung's request id names
the rung (`<output>-<rung id>`).

    rendition.presets {}  -> {presets: [{id, title, group, request: RenditionRequest, ladder?: [RenditionRequest], cost?: Cost}]}
        Built in: youtube-1080p30, youtube-720p30, facebook-720p30, twitch-1080p60,
        twitch-720p30, audio-only-aac, abr-ladder-4 (1080p, 720p, 480p, 360p),
        abr-ladder-3 (720p, 480p, 360p), copy. Only presets this machine can
        make are listed; the rest come back with `available: false, why`.
        `cost` is the governor's estimate of the whole preset on this
        machine (every rung, scaling and sound), so the page's Free, Light
        and Heavy badge is measured rather than guessed.

    rendition.plan {scope?: "programme" | "channel:<id>"}
        -> {nodes: [{id, kind, serves: [request ids], encoder?, reason: {code, text}, cost}],
            totals: {cpu_millicores, devices: {<device>: {millis, sessions}}, egress_kbps}}
        `serves` lists output ids (for a channel's plan, destination ids), each
        once, even where a node works for several rungs of one ladder.
        `encoder` is the catalogue id string, `h264-videotoolbox`.

    governor.status {}
        -> {calibrated_at, fingerprint, cpu: {cores, used_millicores, room_millicores},
            devices: [{id, kind, used_millis, room_millis, sessions_used, sessions_max}],
            egress_kbps, shed: [{what, why}]}
    governor.calibrate {} -> {started: true}    (refused while anything is on air, with data.action to confirm)

A request the governor refuses is an RpcError with code Safety and
`data: {need: Cost, have: Cost, advice: [{text, request: RenditionRequest}]}`,
so the page can offer each piece of advice as a button that retries with that
request.

Events: `event/rendition.plan` {scope, plan} when a plan changes;
`event/governor.shed` {what, why} when something is shed.

## HLS

Served from the control port, no new port:

    GET /hls/<output id>/master.m3u8       multivariant playlist (ABR) or media playlist
    GET /hls/<output id>/<rung>/index.m3u8
    GET /hls/<output id>/<rung>/<n>.m4s     CMAF fMP4 segments, init.mp4, LL-HLS parts

An HLS output is `output.add {type: "hls/output", rendition: {preset: "abr-ladder-4"}, params: {segment_ms, part_ms, window}}`. Segments live
in memory (a ring per rung, sized from the window) with no disk unless
`record` is asked. The same token rules as the rest of the control port;
a playlist URL may carry `?token=` for players that cannot send a header.

## Performance, measured and reported by each agent

* Copy stays a copy: an output with no rendition, or one the source already
  matches, costs what it costs today.
* One decode, one scale per size, one encoder per distinct rendition,
  shared by every output that wants it (the planner's rules, now real).
* Adding or removing an output changes only the nodes the diff names; the
  programme and every other output drop nothing. Prove it with a test.
* HLS: CPU for packaging a four rung ladder, memory per rung for a 30 s
  window, time from a keyframe to its segment being served.
