# What GodwinMix still lacks against vMix, Wirecast, OBS and the servers

The backlog for closing every gap a switching user would hit, with where each
piece goes. It starts from the feature check of 25 October 2026
(`social-films/research/parity.md`, outside this repository), the check
against `main` at v0.3.19 on 29 October, and the teleprompter, which the
first check found nothing on.

**Built in** means it goes in the core or the binary, because it sits on the
frame path, touches keys or the protocol, or every show needs it. **Plugin**
means a first party plugin under `plugins/`, because it is platform specific,
heavy, or wanted by some shows only. Native Rust wherever there is a choice,
nothing that runs unless asked, and each item has to run on a Raspberry Pi 4
or say plainly why it cannot.

Mark an item done only when it is merged with its tests and its how to page.

## Sound

| Item | Today | Where it goes | State |
|---|---|---|---|
| Gain, EQ, gate, expander, compressor, limiter, delay, noise suppression | the OBS names were imported and did nothing | built in, `plugin/filters/audio/` | done in 0.3.20 |
| A sound filter chain per source in the audio panel | API only | built in, UI | done in 0.3.20 |
| Audio delay per source | whole programme only, at restart | built in, `delay/filter` | done in 0.3.20 |
| Buses: a master fader, solo, cue and a headphone bus | fader, mute and meter per source | built in, `buses/` | done in 0.3.20 |
| Mix minus for each guest | none | built in, with the guest return | done in 0.3.20, as a bus with `minus`; one per guest made with each guest's return on `feat/guest-invites` |
| More than two channels | stereo only | built in | |
| VST3 and LV2 effects | none | plugin, out of process | |
| Loudness to a target (EBU R128, LUFS meter) | peak meters | built in | |

## Production

| Item | Today | Where it goes | State |
|---|---|---|---|
| Teleprompter: a page for a phone, tablet or a monitor behind glass, mirrored, several screens in step, speed from the desk, a pedal, a Stream Deck or an agent | none | built in, `prompter.*` methods and `/prompter/` | done in 0.3.20 |
| Teleprompter that follows the speaker's voice | none | plugin | |
| Instant replay and slow motion, several angles | none; the `esports` preset names a `replay` plugin that does not exist | built in ring of encoded frames, replay as a source | on `feat/replay` |
| Replay buffer: save the last minutes to a file | none | built in, the same ring | on `feat/replay` |
| ISO recording of every input, separate audio tracks | only channel feeds | built in, the recording output with a source as its target | |
| Guest invites with their own key, a return feed and talkback | one way WHIP by link, carrying the channel key | built in, `guest.*`, `/guest/<id>/whip` and `/whep` | on `feat/guest-invites` |
| Downstream keys or overlay channels that survive every take | a graphic has to be in each scene | built in | |
| Aux outputs: one source out on its own | outputs carry the programme | built in | |
| Multiview layouts, and multiview out over NDI or to a screen | one fixed mosaic in the browser | built in | |
| Vertical and horizontal from one show | a second show sharing the cameras | built in, a second canvas | |
| Colour correction, LUTs, sharpen, masks | the OBS names are imported and do nothing | built in filters, GL where there is a GPU | |
| Virtual camera moves inside a set | 2D sets only | built in | |
| 29.97 and 59.94 canvases | whole number rates only | built in | |
| HDR and 10 bit | 8 bit I420 | built in, behind a canvas setting | |
| Closed captions: CEA-608 and 708 in the encode, live captions from speech | none | embedding built in, speech to text a plugin | |
| Chat from every platform on screen and in one panel | none | plugin | |
| Signing in to YouTube, Facebook and Twitch to set titles | stream keys only | plugin | |
| Playout: a gapless scheduled list | the Playout plugin, from the gallery | first party plugin in the release | |

## Devices

| Item | Today | Where it goes | State |
|---|---|---|---|
| Virtual camera out, for Zoom and Teams | none | plugin, one backend per OS | |
| PTZ control: VISCA over IP, ONVIF moves, presets | ONVIF discovery only | plugin, `ipcam` and a `ptz` plugin | |
| SDI and DeckLink out | the DeckLink plugin is a source only | plugin, `decklink` | |
| Window capture by handle | a region where the window sits | plugin, `screen` | |
| Game capture on Windows | none | plugin, `screen` | |
| NDI PTZ and tally | none | plugin, `ndi` | |
| Stream Deck and Companion in the release downloads | built from source with npm | release packaging | |
| MIDI controllers without Companion | through Companion only | plugin | |

## Server and headend

| Item | Today | Where it goes | State |
|---|---|---|---|
| ABR ladders on headend channels | one rendition | | done in 0.3.19 |
| More than one GPU | one | | done in 0.3.18 |
| DVR, timeshift and catch up | live window only, 600 s at most | built in, the HLS output | |
| SCTE-35 cues in and out | none | built in, the TS muxer and HLS | |
| Teletext, subtitles and CEA-608 passed through | detected, dropped | built in, the TS muxer | |
| Signed playback URLs and IP allow lists | a static viewer key | built in | |
| DRM | none | plugin | |
| Origin and edge, clustering | none | built in, with nodes | |
| Several tenants on one station | station wide tokens | built in | |
| RIST Main Profile and encryption | Simple Profile | built in | |
| RTSP server with a login per stream | programme only, no login | plugin, `rtsp` | |
| MPEG-2 encode, E-AC-3 conversion, AV1 ingest sized | not yet | built in, the codec catalogue | |
| Per channel metrics in Prometheus | in the API only | built in | |
| Browsing and playing back recordings | none | built in, UI | |

## Performance

| Item | Today | State |
|---|---|---|
| CPU and memory published per release for a Pi 4, a Pi 5, an N100, a laptop with no GPU and an NVIDIA desktop | none published | |
| The cost of each new feature measured on a Pi and written beside it | the chroma key and transitions only | |
