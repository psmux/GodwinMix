# Write a preset

**Presets do not exist yet as a shareable object.** There is no `gmx preset`
command and no preset file format. What exists today is a config file, which
does most of the same work and can be copied from one machine to another.

## What is planned

A preset will be a named bundle of the things a particular kind of operator
needs: a canvas and output preset, a set of scene layouts, source slots waiting
to be filled in, and defaults for the ad library and the stall behaviour. The
first three being built are a church preset, a school preset and an esports
preset, because those are the three deployments the design has actually been
argued against.

The commands will be:

```sh
gmx preset list
gmx preset apply church
gmx preset save my-church        # what this mixer is doing now, as a preset
```

A preset is a file you can mail to somebody. It is not a plugin and it does not
run code, which is the point: a volunteer who is handed a preset and a two
minute video should be able to put a service on air without opening a composer.

Presets land with the scene document (roadmap Phase 5 pulled against Phase 3),
because a preset without scenes is only a config file, and the repository
already has one of those.

## What you can do today

Copy a config. The whole state a mixer starts with is one TOML file.

```sh
# On the machine that is set up the way you like:
cp /etc/godwinmix/godwinmix.toml church-preset.toml
```

Then strip out anything specific to that building: the stream key in the
output URI, the camera addresses, the media directory if it differs. Leave the
canvas, the programme settings, the multiview size, the stall behaviour and the
hardware pinning, which are the parts that carry the knowledge.

```toml
# church-preset.toml: 720p30, a low bitrate for a church uplink, a mosaic small
# enough for a volunteer's laptop, and a long stall timeout because the camera
# at the back of the hall is on Wi-Fi.

[canvas]
width = 1280
height = 720
fps = 30

[program]
video_bitrate_kbps = 2500
audio_bitrate_kbps = 128
keyframe_interval_secs = 2
audio_ramp_ms = 250        # a slower fade: a congregation notices a click

[multiview]
enabled = true
width = 960
height = 540
fps = 8

[stall]
restart_after_secs = 15    # do not thrash a camera on a flaky link
hold_last_frame = true     # a frozen picture beats a black one mid sermon

[[sources]]
id = "cam-wide"
name = "Wide"
uri = "rtmp://127.0.0.1:1935/live/cam-wide"
stall_timeout_secs = 4.0

[[sources]]
id = "cam-lectern"
name = "Lectern"
uri = "rtmp://127.0.0.1:1935/live/cam-lectern"
stall_timeout_secs = 4.0
```

Every key in that file, with its default and what it does, is in
[the configuration reference](../reference/configuration.md).

Hand that file to the next building along with one instruction: change the two
camera addresses and add your output. That is a preset in every respect except
that the mixer does not know the word yet.

## Sharing one

There is no index to publish to yet. Put it in a gist or a repository, and if
it is good, open an issue: the first presets that ship with the mixer will be
ones somebody was actually running, not ones invented at a desk.
