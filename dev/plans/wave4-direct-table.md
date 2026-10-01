# Wave 4: the direct table, between the station and the direct host

Written by showmodel (the station's side) for directhost (the host in
plugins/ingest/src/direct/). It is the one contract between them. Change it
here first, in its own commit, if a shape has to move, and say so to the
other side.

It works the way the channel table works today
(`crates/godwinmix/src/channels/handover.rs`): the station owns the settings
and lays them over the plugin's own with `set_extra("ingest", "direct", ..)`,
then calls `configure` on the running plugin. The plugin answers with events,
which the station routes by prefix to its show registry, the way
`channel.*` events reach `crates/godwinmix/src/channels/events.rs`.

## What the station hands over

`[plugins.settings.ingest].direct`, an array with one row per show whose
input the host should open. The whole table every time, never a diff: the
host compares it with what it runs and touches only the rows that changed.
TOML has no null, so a field that does not apply is left out, never null.

    direct = [
      {
        id = "bbc-one",                 # the show's id, a slug
        name = "BBC One",               # for logs only
        input = {
          uri = "udp://@239.1.1.1:5000",
          program = 101,                # MPEG-TS program; absent: the first
          params = { interface = "en0" },   # per transport; absent: none
          backup = { uri = "srt://10.0.0.2:9000?mode=caller" },  # absent: none
        },
        outputs = [
          { id = "copy", platform = "custom", url = "srt://10.0.0.9:9000", stream = "main" },
          { id = "yt", platform = "youtube", url = "rtmp://a.rtmp.youtube.com/live2/KEY",
            stream = "main", rendition = true, video = "...", audio = "..." },
        ],
        transcode = [ ... ],            # absent when no output converts
        monitor = { alarms = true, pictures = false },
      },
    ]

* A row is in the table when the show exists, is not stopped, and has an
  input. A show that is stopped, or has no input, has no row. A row that
  goes away means stop everything for that show at once.
* `input.uri` is any of the schemes in wave4-contract.md (udp, rtp, srt,
  rtmp, rtsp, http(s) HLS, file, rist) or `channel:<app>/<stream>`, which is
  a stream already in the host's own hub. `input.params` passes through
  untouched from the person; keys the host does not know are ignored and
  logged once. `backup` has the same shape as `input`, minus its own backup.
* `outputs` lists only outputs that are switched on. Each row is exactly a
  channel destination row (`destination_table` in channels/destinations.rs),
  so the restream code runs it unchanged: `id`, `platform`, `url` (the whole
  address, key and all), `stream` (always `"main"`, the input's one stream).
  `platform` is a channel platform (`youtube`, `facebook`, `twitch`,
  `custom` for RTMP, `srt`) or, for outputs a channel never has, the
  scheme itself: `udp`, `rtp` or `rist`, with the whole address in `url`
  (`udp://239.2.2.2:5000`, unicast or multicast). Dispatch on `platform`,
  or on the scheme of `url`; they agree.
  An output that copies has nothing else. One that converts has
  `rendition = true`, and `video` and `audio` from the plan when the input is
  live, exactly as a channel destination's row does; `rendition = true` with
  no `video` means the plan waits for the input's shape, so send nothing yet.
* `transcode` is exactly a channel row's `transcode` (the plan's nodes for
  the input's one stream, `channels/transcode/spec.rs`). The station plans
  it with the same planner and governor the channels use, from what
  `direct.input` said about the input.
* `outputs` may be empty. The input is still opened and published into the
  hub. That is how a show that composites reads its input: the station
  keeps the row with no outputs, and the show's one source reads the hub.
* `monitor.alarms`: run the black, freeze and silence checks (keyframes
  only for video, audio at a low rate). `monitor.pictures`: someone is
  looking, make thumbnails. Both off: demux only, decode nothing.

## What the host says back

Events on the plugin's event channel, all under the prefix `direct.`. The
station ignores fields it does not know, so the host may add some.

`event/direct.input`, when the input goes live, goes idle, switches to or
from its backup, or its shape changes. Same fields as `channel.stream`, so
the station plans from it with the code channels use:

    { show = "bbc-one", state = "live" | "idle",
      since_ms,                       # unix ms when this state began
      from = "239.1.1.1:5000",
      backup = false,                 # true while the backup feeds it
      relay = "127.0.0.1:41935",      # where a hub reader asks for it
      stream = "<hub app>/main",      # what a hub reader asks for
      video = { codec, width, height, fps, kbps },
      audio = { codec, channels, sample_rate, kbps } }

`relay` and `stream` are what an `ingest/rtmp` source in a show is given
(`params.relay`, `params.stream`), so the station needs nothing else to turn
the input into a source. The hub app name is the host's choice; it must not
be a name a channel could have (a channel app is a slug, so anything with a
character outside `[a-z0-9-]` is safe).

`event/direct.output`, when an output's state, error or reconnect count
moves. Same fields as `channel.destination`:

    { show, output, state = "waiting" | "connecting" | "live" | "reconnecting" | "failed",
      since_ms, kbps, reconnects, error }

`event/direct.health`, when a show's health state or its set of alarm kinds
changes, never for a number alone:

    { show, health = { state = "ok" | "warning" | "alarm" | "off",
                       alarms = [ { kind, since_ms, detail } ] } }

`kind` is one of `no-input`, `stall`, `black`, `freeze`, `silence`,
`cc-errors`, `loss`, `output-failed` (the station adds `governor-refused`
and `shed` itself). `since_ms` is unix ms.

`event/direct.stats`, once a second while the table has any row, every show
in one event, counters only (nothing is decoded to make it):

    { shows = [
        { id = "bbc-one",
          input = { kbps, fps, width, height, video_codec, audio_codec, audio_channels,
                    cc_errors, packets_lost, keyframe_ms, last_frame_ms },
          outputs = [ { id, state, kbps, reconnects, encoder, cpu_millicores } ] } ] }

`keyframe_ms` is the interval between the last two keyframes,
`last_frame_ms` how long ago the last frame arrived. A field the host cannot
tell yet is left out. `rendition_text` is the station's to fill from the
plan, so the host does not send it.

## Who does what

The station: the show registry and its file, keys sealed in the secret
store, planning and admitting renditions, the table, aggregating the events
into `show.list`, `show.stats` and `event/show.health`, the compositing
toggle. The host: everything a row asks for, and the four events.
