# Instant replay and the replay buffer

GodwinMix keeps the last minute or so of each angle you pick in memory, as
the encoder wrote it. You mark a moment in and out, play it back at full,
half or quarter speed while you change angle, and put it on air like any
other source. You can also save a moment, or the whole buffer, to an MP4 in
the media folder, which is what OBS calls the replay buffer.

Nothing is recorded until you turn replay on.

## Turn it on

1. View > Replay opens the Replay panel.
2. Tick the angles to keep: **Programme** and any of your sources.
3. Set how many seconds each keeps (60 unless you change it, up to 1800)
   and press **Start replay**.

Each angle then shows how much it holds, in seconds and megabytes, and what
encodes it. A source angle is encoded with the encoder this machine uses for
the programme, with a keyframe every second, so on a Mac it is VideoToolbox
and on a Pi the software encoder. The programme angle encodes nothing: it
keeps what the programme encoder already makes for the outputs.

The same from the command line, an agent or a config file:

```sh
gmx ctl rpc replay.start '{"seconds": 60, "angles": ["programme", "cam1", "cam2"]}'
```

```toml
[replay]
seconds = 60
angles = ["programme", "cam1", "cam2"]
```

With `[replay]` in the config, replay starts with the mixer. Turned on from
the panel, it is kept in the runtime store and comes back after a restart.

## How much memory it takes

A ring holds the angle's encoded picture and sound, so its size is the
bitrate times the seconds. At the programme's default 6 Mbit/s that is about
45 MB a minute per angle. A Raspberry Pi 4 with 4 GB holds several angles
for a minute each. Set `bitrate_kbps` in `[replay]` to encode the source
angles at a lower bitrate than the programme.

## Mark a moment

* **Mark in** (or the I key) opens an event now.
* **Mark out** (or O) closes it.
* **Last 10 s** makes an event of the ten seconds just gone.

Events are listed newest first with their length. Click a name to rename it,
and × to forget it. An event is a stretch of time, so it plays on every
angle.

## Play it

1. Pick a speed: **1×**, **0.5×**, **0.25×**, or anything from 0.1 to 1 on
   the slider.
2. Pick the angle to start on.
3. Press **Play** on an event to load it, or **Air** to load it and put it on
   the programme. **Play** at the top (or P) plays the last event you marked.

The replay plays on a source called `replay`, added the first time. It is a
source like any other: take it, put it in a scene, or take it with a
transition, a stinger among them. At the end it holds its last frame.

While it plays, picking another angle switches to it from the moment on
screen, and another speed carries on from there too. Slow motion is silent,
and the programme gets a picture from it at its own frame rate, as from a camera.

From the command line:

```sh
gmx ctl rpc replay.mark '{"seconds": 8, "name": "Goal"}'
gmx ctl rpc replay.play '{"event": "replay-1", "speed": 0.5, "take": true, "transition": "fade"}'
gmx ctl rpc replay.set '{"angle": "cam2"}'
```

`source.seek` on the `replay` source moves to any frame of it.

## Save it

**Save** on an event writes that event to `Replays/` in the media folder.
**Save buffer** writes everything the picked angle holds. Nothing is
encoded again, so a minute of 1080p is written in the time the disk takes to
take 45 MB. The file shows in the Media panel under Replays and can be added
as a clip.

```sh
gmx ctl rpc replay.save '{}'
gmx ctl rpc replay.save '{"event": "replay-1", "name": "goal"}'
```

A saved event starts at the keyframe at or before its in point, so on a
source angle it can start up to a second early.

## A Stream Deck

The Companion module has Replay: mark in or out, Replay: play, Replay:
change angle or speed, and Replay: save the buffer. See
[Put GodwinMix on a hardware panel](companion-and-streamdeck.md).

## Turn it off

**Stop replay** takes every branch out, frees the memory and forgets the
events. The programme and the sources are left exactly as they were before
replay was on.

See [the reference](../reference/replay.md) for every method and field.
