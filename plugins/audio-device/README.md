# audio-device

A microphone, a line input, a mixing desk feed or the sound side of a webcam,
as a source. Sound only.

## What you need

* A sound input the operating system can already see. If it shows in the
  system's sound settings, this will find it.
* GStreamer 1.24 or later. On Debian and Raspberry Pi OS, `gstreamer1.0-pipewire`
  or `gstreamer1.0-pulseaudio`; on macOS `brew install gstreamer`; on Windows
  the GStreamer runtime installer.
* On macOS and Windows, microphone permission for the program that started the
  mixer. If nobody answers the prompt the input stays silent.

## Three steps

```sh
./plugins/audio-device/build
gmx plugin add ./plugins/audio-device
gmx ctl source add desk --type audio-device/source
```

With no settings at all that is the machine's first input at unity gain. To
pick a particular one, get its id from the `list_audio_inputs` tool and:

```toml
[[sources]]
id = "desk"
type = "audio-device/source"
params = { device = "Scarlett 2i2 Analog Stereo" }
```

## Settings

| Setting | Default | What it does |
|---|---|---|
| `device` | the first input | the id from `list_audio_inputs`, the name the system shows, or a number from 0 |
| `gain_db` | 0 | lift or cut before the mix. Applies while the source is live |
| `muted` | false | silence without stopping. The device stays open, so unmuting is instant |
| `label` | empty | a name for the operator |
| `element` | automatic | force one capture element. Only for a driver bug |

Gain and mute also move through `audio.set`, which is what a fader in the UI
and an agent both use. Setting one leaves the other alone, and the full state
comes back in the answer.

## What it uses, per platform

| Platform | Element | Why |
|---|---|---|
| Linux | `pipewiresrc`, then `pulsesrc`, then `alsasrc` | PipeWire is what a current desktop runs, and it is the only one that can take an input another program already has |
| macOS | `osxaudiosrc` | Core Audio |
| Windows | `wasapi2src`, then `directsoundsrc` | WASAPI 2 first. It has open stutter bugs (gstreamer#2870 and #3339); DirectSound is the older path that does not stutter |

If Windows sound stutters, set `element` to `directsoundsrc` and restart the
source. That is what the setting is for.

## The format

F32LE, 48 kHz, stereo, ten milliseconds a buffer. That is the media contract,
and it is what every source in the mixer sends. A mono microphone is carried as
two identical channels; a 44.1 kHz device is resampled. The plugin asks the
driver for ten millisecond buffers and splits them afterwards if the driver
would not listen.

## Meters

There are none here. Levels are the core's, measured where the sound reaches
the mix, which is the only place a number means what an operator thinks it
means. They arrive in `agent.state` and on the UI's faders.

## When it fails

| What you see | What to do |
|---|---|
| `no data from the input after 5 s` | another program has the device, or permission was refused |
| silent, but healthy | check the device's own input gain in the operating system's sound settings. `gain_db` trims what the device sends; it cannot invent what was never there |
| `no device matches '...'` | the message lists every input this machine has. Copy one of those ids |
| it stutters on Windows | set `element` to `directsoundsrc` |

## Removing it

```sh
gmx ctl source remove desk
gmx plugin remove audio-device
```

## See also

* [Use a webcam](../../docs/how-to/use-a-webcam.md), which ends with adding the camera's microphone this way.
* [The first party plugins](../../docs/reference/plugins.md).
