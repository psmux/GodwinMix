# godwinmix-ptz

PTZ camera drivers for GodwinMix behind one trait, with a fake camera for
each protocol that keeps a position and answers queries. Blocking std
networking, no async runtime, no media stack: `cargo test -p godwinmix-ptz`
needs nothing installed.

```rust
use godwinmix_ptz::{open, Protocol, Target, Velocity};

let mut cam = open(&Target::new(Protocol::ViscaIp, "192.0.2.20"))?;
cam.move_at(Velocity::new(0.3, 0.0, 0.0))?;   // pan right at 30 %
cam.stop()?;
cam.save_preset(1)?;
let p = cam.position()?;                      // pan, tilt -1..1, zoom 0..1
```

`second()` gives another driver on the same camera for use from another
thread while the first is busy: ONVIF, Canon and the fake have one, VISCA
does not (one sequence count, and often one TCP link). The plugin moves the
camera through one and sends slow commands through the other.

The surface is in [docs/reference/ptz.md](../../docs/reference/ptz.md). The
plugin that uses it is [plugins/ptz](../../plugins/ptz/README.md).

## Protocols

| Slug | What | Port | Login |
|---|---|---|---|
| `visca-ip` | Sony VISCA over IP: an eight byte header (type, length, sequence number) in front of each VISCA message, on UDP | 52381 | none |
| `visca-tcp` | raw VISCA bytes on a TCP connection, as PTZOptics, Lumens, Marshall and many others offer | 5678 by default here; the maker's manual has theirs | none |
| `onvif` | ONVIF PTZ (ver20) and imaging (ver20) over SOAP | 80 | WS-Security UsernameToken, password digest |
| `canon` | Canon's XC protocol, the HTTP API of the CR-N series | 80 | HTTP Basic or Digest, whichever the camera is set to |
| `fake` | a head in memory, no network | | |

### VISCA

Followed: Sony's VISCA command lists for the BRC and SRG series cameras
(pan and tilt drive `8x 01 06 01`, absolute `8x 01 06 02`, zoom `8x 01 04 07`
and `47`, focus `08`, `38` and `18`, presets `8x 01 04 3F`, exposure `39`,
`4A`, `4B`, `4C`, inquiries `8x 09 06 12` and `8x 09 04 47`) and Sony's
VISCA over IP specification for the header, the RESET control command
(`02 00`, payload `01`) and the `0F 01` sequence error. Every byte layout is
asserted against a literal in `src/visca/tests.rs`.

* A command is settled by its acknowledgement. Over IP the reply's sequence
  number must match; over raw TCP, which has none, a late completion from an
  earlier command is never taken for a new command's answer.
* A lost datagram is sent once more with the same number, then reported as a
  timeout naming the port. A `sequence number abnormal` answer resets the
  counter and sends again.
* Preset n is VISCA memory number n minus 1, which is Sony's numbering.
* Speeds: pan 1 to `0x18`, tilt 1 to `0x14`, zoom and focus 0 to 7. Positions
  are normalised over `Ranges`, whose defaults (pan plus or minus `0x0990`,
  tilt `0x0510` up and `0x01B0` down, zoom `0x4000`) suit most 20x and 30x
  cameras; set them per camera when yours differ.

Not checked on hardware yet: whether every camera answers on the sender's
port (some documents say the controller should use 52381 too), and the
exposure step values, which are the camera's own.

### ONVIF

Followed: the ONVIF PTZ Service Specification and Imaging Service
Specification (ver20 WSDLs): `ContinuousMove`, `Stop`, `AbsoluteMove`,
`GetStatus`, `GetPresets`, `SetPreset`, `GotoPreset`, imaging
`SetImagingSettings`, `Move` and `Stop`. Speeds and positions are in the
generic spaces every ONVIF PTZ camera supports. A GodwinMix preset n is the
camera preset named `preset-n` (or whose token or name is `n`). The SOAP
envelope, the WS-Security digest and the HTTP call are shared with the ipcam
plugin's discovery and tested against the ONVIF Application Programmer's
Guide's worked example. One push focus and the priority exposure modes have
no ONVIF equivalent and say so.

### Canon CR-N

Canon publishes the protocol, the "XC Protocol Specifications", only through
its developer programme, and no public copy could be read. What this driver
follows, and where each part comes from:

* Canon, *CR-N500 / CR-N300 Settings Guide*, firmware 1.7.0 (English): the
  HTTP server's Basic or Digest authentication and ports, user rights
  ([Camera Control]), and up to 100 presets.
* The open source integrations that cite the spec page by page: Bitfocus
  Companion's `companion-module-canon-ptz` (v2.9.1, which follows "XC
  Protocol Specifications BPE-7216-011"), `cam_control` (which cites "XC
  Control Protocol Specifications BPE-7216-005" by page), and `ptz-pilot`.
* Models this is meant for: CR-N100, CR-N300, CR-N350, CR-N400, CR-N500,
  CR-N700 and CR-X300. The defaults are the CR-N500's.

What it sends, all GET under `/-wvhttp-01-/`:

| Control | Request |
|---|---|
| pan and tilt at a speed | `control.cgi?pan=right&pan.speed.dir=<10..10000>&tilt=up&tilt.speed.dir=<n>` (hundredths of a degree a second) |
| stop | `pan=stop&tilt=stop&zoom=stop` |
| zoom | `zoom=tele\|wide&zoom.speed.dir=<0..15>`, `zoom=stop` |
| focus | `focus=auto\|manual`, `focus.action=near\|far\|stop&focus.speed=<0..2>`, `c.1.focus.action=one_shot` |
| preset | `control.cgi?p=<n>` to recall, `preset/set?p=<n>&name=...&ptz=enabled&focus=enabled&exp=disabled&wb=disabled&is=disabled&cp=disabled` to save |
| position | `info.cgi?item=c.1.pan,c.1.tilt,c.1.zoom` |
| go to | `control.cgi?pan=<0.01 degrees>&tilt=<n>&zoom=<n>` |
| exposure | `c.1.exp=auto\|tv\|av\|manual`, `c.1.me.shutter.mode=speed&c.1.me.shutter=<n>`, `c.1.me.diaphragm...`, `c.1.me.gain...` |

The answer is `name:=value` lines, `==?` when the camera rejected a value in
its current mode, and the outcome is the `Livescope-Status` header (0 is
success; 301, 401, 403, 406, 503, 507 and 509 are each turned into a message
saying what to do). Requests are kept 30 ms apart, as the spec asks.

Unverified without a camera: the zoom speed range (some sources say 0 to 15
on the CR-N500, others 0 to 127; 15 is used because every model takes it),
the zoom position's unit and direction (taken as low at tele, the CR-N500's
570 to 7300), whether the short keys (`pan=`) and the full ones (`c.1.pan=`)
are interchangeable on every firmware, and the exposure value units
(shutter as the denominator, iris as F number times 100, gain as dB times
10, per Companion's module).

## Tests

```sh
cargo test -p godwinmix-ptz
```

`tests/second.rs` checks that a second ONVIF and Canon driver work beside
the first on the same fake camera, and that VISCA gives none.

Every driver runs the same checks through the trait against its fake on
loopback (`tests/drivers.rs`): move, stop and stay stopped, absolute moves,
a preset saved and recalled, zoom, focus and exposure landing on the head,
and the errors for a wrong login and a refused command. The fakes decode
with their own code, written from the protocol documents, so a wrong byte
shows up as the head in the wrong place.
