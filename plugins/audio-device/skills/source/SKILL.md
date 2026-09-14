---
name: audio-device-source
description: Add, configure and diagnose a sound input in GodwinMix (microphone, line input, mixing desk, the audio side of a webcam). Use this when the operator asks for a microphone or "the sound from the desk", when a source is silent or too quiet, and when setting gain or mute on an input.
---

# Audio device sources

One instance is one sound input. It carries no picture: a camera's picture is
`camera/source`, and a webcam that has both appears here and there separately.

## Find out what is there first

Call `gmx_audio_device_list_audio_inputs` with no arguments:

```json
{"inputs": [{"id": "alsa_input.usb-Focusrite_Scarlett", "name": "Scarlett 2i2 Analog Stereo", "api": "pipewire"}]}
```

`id` is what goes in the source's `device` setting. An empty list means nothing
is plugged in, or another program has the device, or the mixer has not been
granted microphone permission on macOS or Windows.

## Add one

```
source.add {id: "desk", type: "audio-device/source", params: {device: "Scarlett 2i2 Analog Stereo", label: "Desk out"}}
```

With no params at all the machine's first input runs at unity gain.

| Param | What it does |
|---|---|
| `device` | the id from `list_audio_inputs`, the name the system shows, or a number from 0. Empty means the first input |
| `gain_db` | lift or cut before the mix. 0 is unity. Applies live |
| `muted` | silence without stopping. The device stays open, so unmuting is instant |
| `label` | a name for the operator |
| `element` | force one capture element. Only for a driver bug |

## Gain and mute while it is live

Use `audio.set` rather than `configure`. It is read back in full:

```
audio.set {gain_db: -6}          ->  {gain_db: -6, muted: false}
audio.set {muted: true}          ->  {gain_db: -6, muted: true}
```

What you leave out is left alone: a mute does not move the fader. Gain is
clamped to -60 to +12 dB rather than refused, and -60 dB is silence.

`layers` is refused. Page and media levels belong to a source with a document
in it, such as a web page, not to a microphone.

## Meters

There are none here. Levels come from the core, measured where the sound
reaches the mix, and `agent.state` carries `audio_peak_db` per source in its
detailed form. Do not ask this plugin for a level; it does not compute one, on
purpose, because a plugin measuring its own output tells you nothing about what
the audience hears.

## When it is silent

| What you see | What to do |
|---|---|
| `health` says `no data from the input after N s` | another program has the device, or permission was refused. On Linux, check `pactl list sources` or `wpctl status` shows it |
| sound, but far too quiet | the device's own input gain is separate from `gain_db`. Raise it in the operating system's sound settings first, then trim here |
| it stutters on Windows | `wasapi2src` has open stutter bugs (gstreamer#2870 and #3339). Set `element` to `directsoundsrc` and restart the source |
| the device disappears when unplugged | the source goes degraded, then failing, and the supervisor restarts it. Plug it back in and it comes back |

## Latency

The plugin asks the driver for ten millisecond buffers and splits them to ten
milliseconds afterwards if the driver would not. That is the media contract.
It declares no latency number, so the core's aligner measures what actually
arrives rather than trusting a figure that would be wrong on the next machine.
