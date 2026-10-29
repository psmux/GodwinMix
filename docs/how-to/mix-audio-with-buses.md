# Mix audio with buses

Every source has a fader, a mute and a meter, and they make one mix: the
programme. This page is for when one mix is not enough. You want your own
headphones to hear a microphone before it goes up, a monitor speaker in the
room to get a different balance, or a remote guest to hear the show without
the echo of their own voice.

Each of those is a bus: a mix of its own beside the programme, with its own
master. Nothing is built for buses until you add the first one, so a show
that never uses them costs exactly what it did before. The keys, methods and
errors are in [the bus reference](../reference/audio-buses.md).

The examples use `gmx ctl rpc` against a mixer with two sources, `host` and
`guest-1`. Point it at your mixer with `--url` or `GODWINMIX_URL`, and add
`--token` when it has one. The same calls work over `/rpc`, over REST at
`/api/v1/audio/...`, and as agent tools.

## The programme master

The master moves everything that goes out, after every source's fader:

```sh
gmx ctl rpc audio.master.set '{"gain": 0.8}'
```

```json
{
  "gain": 0.8,
  "muted": false,
  "should_retry": true,
  "trace_id": "d422fe0843b7239e383c18542c128958"
}
```

`gain` is the same scale as a source's fader: 0.0 silent, 1.0 unity, up to
10.0. `{"muted": true}` mutes the programme. The master is saved with the
rest of the desk and comes back after a restart.

In the web UI it is the **Master** row at the top of the Audio panel.

## A guest's mix minus

A guest on a call needs to hear the show, but not themselves: their own voice
coming back a second late makes talking impossible. A mix minus bus is the
programme without one source:

```sh
gmx ctl rpc audio.bus.add '{"id": "guest-return", "minus": "guest-1"}'
```

```json
{
  "id": "guest-return",
  "kind": "minus",
  "listen": "/pcm/bus:guest-return",
  "minus": "guest-1",
  "should_retry": false,
  "trace_id": "e0f962566687185eb57444fd985f07a1"
}
```

The bus follows the programme as it is heard: whoever is on air, with every
take and crossfade, at their faders. The one difference is that `guest-1` is
never on it. It is fed ahead of the programme master, so muting the programme
does not cut the guest off.

To send it to the guest, make it the sound of an output with `audio`, for
example an SRT or RTMP destination the guest's software plays:

```toml
[[outputs]]
id = "guest-1-return"
uri = "srt://guest-host.example:9000"
audio = "guest-return"
```

The output gets the programme's picture and the bus's sound. A bus that an
output reads cannot be removed until the output is pointed back at the
programme or removed, and the refusal says so.

One bus per guest: add `guest-2-return` with `minus = "guest-2"`, and so on.

## Headphones with cue and solo

A cue bus carries the sources you mark, at unity, and the programme while
nothing is marked. It is what the operator's headphones listen to. Add one
and play it on a sound card on the mixer's machine:

```sh
gmx ctl rpc audio.device.list
```

```json
{
  "devices": [
    {
      "id": "BuiltInSpeakerDevice",
      "index": 0,
      "name": "MacBook Pro Speakers"
    }
  ],
  "trace_id": "5447f90b80014862a5e4b6079b316b98"
}
```

```sh
gmx ctl rpc audio.bus.add '{"id": "phones", "cue": true, "device": "default"}'
```

`device` takes `default` (the system's choice), or a device's `id`, `name` or
`index` from the list. The answer says `"device_state": "opening default"`
while the card is being opened, and the field goes away once it plays. If the
card cannot be opened, `device_state` says why and the rest of the show is
untouched: the card plays in a pipeline of its own.

Now listen to the guest before you put them on air:

```sh
gmx ctl rpc audio.cue.set '{"source": "guest-1", "cue": true, "solo": true}'
```

The answer is the whole desk, with `"cue": ["guest-1"]`. `solo` clears every
other mark first; leave it out to add a source to what you already hear. A
source marked cue is heard even while it is muted, so you can check a
microphone with its mute on. Nothing on air changes. `"cue": false` takes the
mark off, and with no marks left the headphones carry the programme again.

Cue marks are not kept across a restart. The bus itself is.

In the web UI, each source in the Audio panel has **Cue** and **Solo**
buttons once a cue bus exists, and the bus strip has a sound card picker.

## A monitor feed with its own balance

A stage monitor or an isolated recording often wants a different balance
from the programme. An ordinary bus carries each source at the level of its
send:

```sh
gmx ctl rpc audio.bus.add '{"id": "monitor", "sends": {"host": 1.0}}'
gmx ctl rpc audio.send.set '{"bus": "monitor", "source": "guest-1", "level": 0.5}'
```

```json
{
  "id": "monitor",
  "kind": "sends",
  "listen": "/pcm/bus:monitor",
  "sends": {
    "guest-1": 0.5,
    "host": 1.0
  },
  "should_retry": true,
  "trace_id": "dd0651d058a8b382f142036eb193fcec"
}
```

A send is after the source's own fader and follows its mute, so pulling a
microphone down pulls it down everywhere. A send at 0 takes the source off the
bus. Unlike the programme, a bus with sends does not follow what is on air:
the host is on the monitor whether or not the host's camera is.

Play it on a sound card with `audio.bus.set`:

```sh
gmx ctl rpc audio.bus.set '{"id": "monitor", "device": "default"}'
```

or record it on its own with an output whose `audio` is `monitor`.

In the web UI, pick **monitor** in the **Bus** list of the Audio panel. The
bus's strip shows its meter, master and mute, and each source gets a **Send**
fader for that bus.

## Hear a bus in a browser

Every bus is a target of the `/pcm/` and `/opus/` monitoring doors described
in [see and hear the mixer](preview-and-audio.md): `/pcm/bus:monitor`,
`/opus/bus:guest-return`. The `listen` field in each answer is the path. The
branch is built when you connect and goes when you hang up.

The Audio panel's **Listen** button plays the picked bus in that browser.

## Change and remove a bus

`audio.bus.set` changes a bus in place, live: `gain` and `muted` (the bus
master), `name`, `minus`, and `device` (an empty `device` stops the sound
card). `audio.bus.remove` takes it out, live, with no gap in the programme's
sound:

```sh
gmx ctl rpc audio.bus.remove '{"id": "monitor"}'
```

When an output still reads it, the answer is a refusal that names the output:

```text
Error: audio.bus.remove: bus monitor is the sound of iso-b. Point it back at the programme with output.set (audio left out), or remove it, then remove the bus.
```

The buses, their sends and the master are saved in the runtime store as they
change, and the next start builds them again. They can also be written by
hand in the config file's `[audio]` table; see
[the reference](../reference/audio-buses.md#the-audio-table).

## What it costs

Nothing until the first bus. Two buses on four sources measured under 2
percent of one core of an Apple M4 Pro; the figures and how to measure your
own machine are in [the reference](../reference/audio-buses.md#cost).
