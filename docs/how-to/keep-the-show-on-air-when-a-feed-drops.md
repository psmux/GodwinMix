# Keep the show on air when a feed drops

A camera feed drops in the middle of a show. Without a backup the programme
holds that camera's last frame for up to 45 seconds and then shows the slate
until the feed comes back. With one, the mixer puts another source in its
place a few seconds after the picture stops, and puts the camera back once it
has been steady again for a few seconds.

This page sets one up, says what happens on air while it runs, and how to
take the camera back by hand.

## Pick a backup

The backup can be any source the mixer already has: a wide shot, a looping
clip, a still picture with the show's name on it, a graphic. Pick one that is
live. Every source in GodwinMix keeps running whether it is on air or not, so
a backup that is live switches instantly and nothing has to start up at the
moment it is needed.

A backup that is down too cannot help. The source settings say so before it
matters, and if it happens during the show the backup's own backup is used,
when it has one.

## Set it up in the page

1. Open the camera's settings from its tile.
2. In **Backup**, under **If this source goes down, show**, pick what to show
   instead. Every other source is in the list by name.
3. Leave **after 3 seconds** unless you want the last frame held longer.
4. Pick the transition: Dissolve is the default, Cut is a hard switch, and
   Wipe, Dip to black, Slide, Push, Zoom and Box are there too. It runs for
   its usual 300 ms; `duration_ms` in the config or over the API changes
   that.
5. Leave **Switch back by itself when it returns** ticked, with **once it has
   been steady for 5 seconds**, or untick it to choose the moment yourself.

Each change is saved as you make it, and the line under the boxes says what
will happen: "Saved. If Cam main goes down, Cam wide goes on after 3 s. It
switches back by itself." If the backup you picked has no picture right now,
a red line says so. **Nothing (hold the last frame, then black)** takes the
backup away.

A camera that goes down with no backup set says **No backup set** on its tile,
with a **Set a backup** button that opens these settings at the right place.

## Set it up in the config

```toml
[[sources]]
id = "cam-wide"
uri = "rtsp://192.168.1.65/stream1"

[[sources]]
id = "cam-main"
uri = "srt://192.168.1.64:9000"
on_loss = { backup = "cam-wide", after_secs = 3, transition = "dissolve", duration_ms = 500, return = "auto" }
```

Only `backup` is needed; the rest are the defaults written out. Every key is
in the [configuration reference](../reference/configuration.md#on_loss). A
backup that is not in the config, a source that is its own backup, and two
sources that back each other up stop the mixer at start with a line saying
which and what to change.

## Set it up from a shell or an agent

```sh
gmx ctl source backup set cam-main cam-wide --after 3 --transition fade
gmx ctl source backup set cam-main --manual     # change one setting, keep the rest
gmx ctl source backup clear cam-main
```

The same call over the API is `source.on_loss.set`, and an agent has the
`set_source_backup` tool. A backup that does not exist is refused with the
ids there are; a loop is refused with the loop spelled out.

## What happens on air

Say `cam-main` is on air in a picture in picture over `cam-wide`, with a
backup of `clip-loop`, and its feed drops.

* For the first two seconds nothing looks different: frames have just
  stopped, and the last one is still on screen.
* After about two seconds the source reads stalled, and its tile and the
  list under the programme monitor say "Cam main lost, backup in 3 s",
  counting down.
* `after_secs` (3 by default) after it read lost, so about five seconds after
  the last frame, `clip-loop` dissolves in where `cam-main` was,
  in the inset, at the inset's size. Every scene that has `cam-main` in it
  shows `clip-loop` instead, the preview of an armed scene too. The tile and
  the programme area say "Cam main lost 0:12, showing Clip loop", with the
  clock running. The tally light of `clip-loop` comes on. If the backup is
  down too, the line says "and its backup is down too", in red.
* `cam-main` comes back. The line says "Cam main is back, returning in 5 s",
  with **Return now** and **Stay on backup**. Five seconds of steady picture
  later it dissolves back in. If it drops again inside those five seconds the
  clock starts over, so a feed that flickers does not flap the programme.

The programme never stops through any of it: the swap is the same transition
a take uses, run on the compositor's own clock, and the encoder cannot tell it
happened.

If you take something else while the backup is on, that is what goes on air.
The backup only ever stands in for `cam-main`; it never makes a take of its
own, so when `cam-main` returns nothing moves unless `cam-main` is on air.

## Take it back by hand

With **Switch back by itself when it returns** unticked (`return = "manual"`),
the backup stays on after the camera is back. The line says "Cam main is back,
Clip loop is still on", with a **Back to Cam main** button. Press it when you
are ready; **Stay on backup** does the same for one outage when the source
would otherwise go back by itself. The buttons never take anything to
programme. From a shell:

```sh
gmx ctl source backup return cam-main
gmx ctl source backup stay cam-main    # keep the backup on, even with return = "auto"
```

A return before the camera has a picture again is refused, because it would
put a frozen frame or the slate on air.

## Watch it from somewhere else

`event/source.backup.on` and `event/source.backup.off` say when a backup goes
on and comes off, and why. The same two names are hooks, so a script can page
somebody when a camera drops:

```toml
[[hooks]]
event = "source.backup.on"
http = "https://chat.example/hooks/camera-dropped"
```

`gmx ctl source list` prints `(lost 0:12, showing clip-loop)` beside a source
on its backup. The [protocol reference](../reference/protocol.md) has the
methods and the events, and [sources](../reference/sources.md#a-backup-that-takes-over)
has the rules in full.
