# Wave 4: every feed is a show, and a show is its properties

The owner's model, agreed: one show is one encoder or transcoder, so 200
channels from a headend are 200 shows. Whether a show composites, and
whether each output copies, repackages or re-encodes, are properties of the
show and of its outputs, set and changed like any other property. One page
watches them all. An agent can make 200 of them in one call.

Read dev/plans/shows-and-renditions.md and dev/plans/wave3-contract.md first;
this extends them. Change this file first, in its own commit, if a shape has
to move.

## A show's properties

    Show {
      id, name, state, on_air, ...                       as today
      compositing: bool,        true: scenes, transitions, a programme encode (today's show)
                                false: one input straight to its outputs, no compositor
      input: InputSpec | null,  what a show without compositing takes; ignored when compositing
      outputs: [ShowOutput],    for a show without compositing; a compositing show keeps output.* inside it
      health: Health,           see Monitoring
    }

    InputSpec {
      uri,                      udp://@239.1.1.1:5000, srt://..., rtmp://host/app/key, rtsp://...,
                                https://.../x.m3u8, file:///clip.ts, rist://..., or a channel
                                stream "channel:<app>/<stream>"
      program?: u16,            MPEG-TS program number for a multi program feed (default: first)
      params?: object,          per transport: interface for multicast, latency for SRT, passphrase...
      backup?: InputSpec,       switched to when the main input stalls; back when it returns
    }

    ShowOutput {
      id, uri | platform + key, enabled,
      rendition: null           copy: the input's own bytes, repackaged into the output's container
               | RenditionRequest | {preset} | {ladder: [...]}    re-encode, planned and admitted
      state, kbps, reconnects, error             as channel destinations report today
    }

A show without compositing runs light: no process of its own. All such
shows live in one shared host (the "direct host"), built on the engine the
channels already use (plugins/ingest: hub, restream, transcode), widened to
take every input above. Its input is demuxed, never decoded, unless an
output asks for a rendition or monitoring asks for a picture (below); then
it is decoded once and shared, through the frame bus where another process
needs it.

`show.set {id, compositing: true}` turns a direct show into a mixed one: the
station starts a show process whose one source is the input (read from the
frame bus, so nothing is decoded twice), and the outputs move to it. `false`
does the reverse when the show has one source and no scenes in use, and
refuses with the reason otherwise. Outputs keep going across the switch;
measure and report the gap.

## Methods

    show.add {name, compositing?: bool = true, input?, outputs?, from?}     as today, plus the new properties
    show.add_many {shows: [ShowAdd], dry_run?: true}
        -> {added: [id], refused: [{index, name, why, data}], plan: {cost, fits: bool}}
        Validated whole first; with dry_run answers what it would do and whether the
        governor would admit every rendition. Applies all that fit; never half a show.
    show.set {id, name?, compositing?, input?}
    show.output.add {show, ...ShowOutput} / show.output.set / show.output.remove      for direct shows
    show.remove_many {ids}
    show.stats {ids?, fields?} -> {shows: [{id, health, input: InputStats, outputs: [OutputStats]}]}
        One read for many shows, cheap enough to call every second for 200.

For a compositing show, `output.*` and `source.*` with `?show=<id>` keep
working as today. For a direct show the station answers `show.output.*`
itself. Every one of these is an MCP tool (see Agents).

## Monitoring

    Health { state: "ok" | "warning" | "alarm" | "off", alarms: [Alarm] }
    Alarm { kind: "no-input" | "stall" | "black" | "freeze" | "silence" | "cc-errors"
                  | "loss" | "output-failed" | "governor-refused" | "shed",
            since_ms, detail }
    InputStats { kbps, fps, width, height, video_codec, audio_codec, audio_channels,
                 cc_errors, packets_lost, keyframe_ms, last_frame_ms }
    OutputStats { id, state, kbps, reconnects, rendition_text, encoder?, cpu_millicores }

Pictures and black, freeze and silence detection cost a decode, so they run
only while someone is looking or an alarm is asked for: a direct show that
only copies decodes keyframes alone (about one a second) for its thumbnail
and its black and freeze checks, and decodes audio at a low rate for
silence. Say what it costs per show and keep it off when the wall is closed
unless the show's alarms are switched on (default on for direct shows,
because a headend operator wants to know a channel went black without
watching).

Events: `event/show.health {id, health}` on every change of state or alarm,
not on every number. Numbers are read with show.stats.

## Agents

MCP tools, bound from the method table: show.list, show.add, show.add_many,
show.set, show.remove, show.remove_many, show.output.add/set/remove,
show.stats, show.start, show.stop, rendition.presets, rendition.plan,
governor.status, project.import, project.export, channel.add,
channel.destination.add, plus what exists. The standard profile's twelve hot
tools are chosen again for the new shape; the rest are behind search_tools.
The MCP byte budget stays within its test.

A skill page skills/godwinmix-operate gains a headend recipe: list the feeds,
show.add_many with dry_run, read the cost, apply, watch show.stats.

## Pages

* The monitoring wall: every show as a row (and a tile view), live
  thumbnail, input numbers, each output's state, alarms with their age, the
  compositing toggle and each output's copy or format, sortable and
  filterable, 200 rows smooth (virtualized), only rows on screen fetched.
* Add shows in bulk: paste a list (one feed per line, or CSV with name,
  input, program, outputs), preview with the dry run, apply.
* The show tabs from wave 3 stay for switching into a mixed show; the wall
  is where many shows are watched.

What the wall reads that the shapes above do not yet name. The wall is
built against these; whoever lands the method moves this text up into its
section, or changes the shape here and the wall follows.

* A show's picture: `GET /api/v1/shows/{id}/thumbnail.jpg?width=160`, with
  the token as `/api/v1/snapshot` takes it. A 404 or 409 while there is no
  picture; the wall draws a placeholder. Fetched only for rows on screen,
  every two seconds, only while the wall is open and the tab visible.
* Totals for the header: `governor.status` answers `ingress_kbps` beside
  its `egress_kbps`. Without it the wall sums the rows it has read and says
  so.
* Alarm thresholds: `show.set {id, alarms: {enabled, black_ms, freeze_ms,
  silence_ms, silence_dbfs}}`, and the same object on Show as `alarms`.
* `show.list` answers `{shows: [Show], current}` as in wave 3, each Show
  carrying `compositing`, `input`, `outputs` and `health` as above.
  `show.stats {ids}` is asked once a second for the rows on screen only.
* `event/show.changed {show}`, `event/show.removed {id}` and
  `event/show.health {id, health}` keep the list current between reads.

## Performance to report

* 200 direct shows, multicast in, one copy output each: CPU and memory of
  the direct host and the station, packet loss, and whether any GOP dropped.
* The same with thumbnails and alarms on, and with the wall open.
* As many 720p transcodes as the governor admits on this Mac, hardware and
  CPU only.
* The compositing toggle: output gap when turning it on and off.
