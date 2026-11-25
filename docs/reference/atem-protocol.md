# The ATEM network protocol, as `godwinmix-atem` speaks it

Blackmagic has not published the protocol its ATEM switchers speak to ATEM
Software Control and to panels. This is this project's own description of
the part `crates/godwinmix-atem` uses, written for GodwinMix.

## Where it comes from, and what it was checked against

* **Source.** Community documentation of the protocol, chiefly the Sofie
  `atem-connection` library (MIT licence), read for packet and block
  layouts. No code was copied from it. Field names here are ours.
* **Versions.** The switcher reports a protocol version in `_ver`. The
  layouts below hold for protocol 2.27 to 2.32, which the community notes
  pair with firmware 7.5.2, 8.0, 8.0.1, 8.1.1, 9.4 and 9.6. Where a layout
  changed inside that range it says so. The plugin's `state` answers both
  the protocol version and the firmware it implies.
* **Models.** The notes are kept by people using the ATEM Mini family and
  the larger Television Studio and Production Studio switchers. The blocks
  used here are the ones every model sends; a model has as many M/Es,
  keyers, aux outputs and media players as its `_top` says.
* **Not yet checked against hardware here.** No capture from a real
  switcher is committed. The fake switcher the tests use
  (`crates/godwinmix-atem/tests/fake/`) is built from this document, so the
  tests prove the client and this document agree, not that this document
  and a switcher agree. A packet capture from a real switcher, with serial
  numbers removed, is the next thing to add.

## Transport

UDP. The switcher listens on port 9910. The client binds any local port,
connects the socket to the switcher, and keeps one session per switcher.
Every number is big endian.

## The packet header

Every datagram starts with twelve bytes:

| Bytes | Field |
|---|---|
| 0 to 1 | flags in the top five bits, the datagram's whole length in the low eleven |
| 2 to 3 | session id |
| 4 to 5 | the packet id this datagram acknowledges, when it is an ack |
| 6 to 7 | the first packet id to resend, when it asks for a resend |
| 8 to 9 | zero |
| 10 to 11 | this datagram's own packet id, when it asks to be acknowledged |

A datagram whose length field differs from its real length is not an ATEM
packet and is dropped.

| Flag (after shifting byte 0 right by three) | Meaning |
|---|---|
| `0x01` | acknowledge me: the datagram carries a packet id |
| `0x02` | hello: opens a session |
| `0x04` | this is a resend |
| `0x08` | please resend from the id in bytes 6 to 7 |
| `0x10` | this acknowledges the id in bytes 4 to 5 |

## Opening a session

The client sends a twenty byte hello: flag `0x02`, length 20, a session id
of its choosing with the top bit clear, then `00 00 00 00 00 3a 00 00 01 00
00 00 00 00 00 00`. Every known client sends exactly those payload bytes;
what bytes 8 and 9 mean is not known.

The switcher answers with its own hello. Its first payload byte is `0x02`
when it accepts. Community notes report `0x03` when every client slot is in
use; the client treats that as a refusal and tries again after five
seconds. The client acknowledges the hello's packet id (zero) and the
session is open.

From then on the switcher's datagrams carry a session id with the top bit
set. The client copies the session id of the latest datagram into
everything it sends.

## Acknowledgements and resends

* Packet ids are fifteen bits: they count from 1 and wrap from `0x7fff` to
  0. An id up to half the space behind another is older than it.
* The switcher's acknowledged datagrams are taken strictly in order. One
  that arrives after a gap is not acknowledged and waits for the switcher
  to send the gap again. A repeat of one already taken is acknowledged
  again, since the first acknowledgement was lost.
* An acknowledgement covers its id and every id before it.
* The client resends an unacknowledged datagram after 200 ms with the
  resend flag set, up to ten times. After that the session is dropped and
  a new one is opened. (The community library resends after 60 ms; 200 ms
  is kinder to a busy switcher and still quicker than an operator notices.)
* A datagram with the resend request flag asks the client to send again
  everything from the id in bytes 6 to 7 that it still holds.

## Staying open, and coming back

The switcher keeps sending acknowledged datagrams, empty ones when nothing
has changed. Five seconds with nothing from it ends the session. While no
session is open the client says hello once a second; after five seconds
without an answer it waits 2, 4 and then 8 seconds between hellos, and
goes back to once a second as soon as a switcher answers. A connected
socket hears an ICMP port unreachable as a refused receive, which the
client reports as "nothing listens on UDP port 9910" rather than "no
answer".

## Command blocks

After the header, a datagram carries zero or more blocks:

| Bytes | Field |
|---|---|
| 0 to 1 | the block's length, these eight bytes included |
| 2 to 3 | zero |
| 4 to 7 | the name, four ASCII bytes |
| 8 on | the body |

The client puts as many command blocks in one datagram as fit in 1400
bytes. The switcher sends its whole state as blocks when a session opens,
ends that dump with `InCm`, and then sends one block per change.

## State blocks read

Offsets are into the body. Text fields are fixed width and end at the first
zero byte.

| Block | Layout |
|---|---|
| `_ver` | protocol major u16, minor u16 |
| `_pin` | product name in 40 bytes, model number u8 at 40 |
| `_top` | counts, one byte each: M/Es, sources, downstream keyers, aux outputs, mix minus outputs, media players. Later bytes moved at 2.30 and are not read |
| `_MeC` | M/E u8, upstream keyer count u8 |
| `InPr` | source u16, long name 20 bytes at 2, short name 4 bytes at 22 |
| `PrgI`, `PrvI` | M/E u8, pad, source u16 at 2 |
| `TrSS` | M/E, style, selection, next style, next selection (u8 each) |
| `TrPs` | M/E u8, in transition u8, frames remaining u8, pad, position u16 at 4 (0 to 10000) |
| `TMxP`, `TDpP`, `TWpP` | M/E u8, rate in frames u8 |
| `KeOn` | M/E u8, keyer u8, on air u8 |
| `FtbS` | M/E u8, fully black u8, in transition u8, frames remaining u8 |
| `FtbP` | M/E u8, rate u8 |
| `DskS` | keyer u8, on air, in transition, auto transitioning, frames remaining (u8 each) |
| `DskP` | keyer u8, tie u8, rate u8, then key settings not read |
| `DskB` | keyer u8, pad, fill source u16 at 2, key source u16 at 4 |
| `AuxS` | aux u8, pad, source u16 at 2 |
| `TlSr` | count u16, then per source: source u16 and a flag byte, bit 0 programme, bit 1 preview |
| `MPrp` | index u16, used u8, unsupported steps u8, name length u16, description length u16, name, description |
| `MRPr` | status u8 (bit 0 running, bit 1 waiting), loop u8, index u16 (`0xffff` when none) |
| `MPCE` | player u8, source type u8 (1 still, 2 clip), still u8, clip u8 |
| `InCm` | the end of the dump |

A block this client does not know is skipped. A known block too short for
its layout is skipped without changing anything, since a newer firmware
adds fields at the end.

## Commands sent

| Command | Block | Body |
|---|---|---|
| programme | `CPgI` | M/E u8, pad, source u16 |
| preview | `CPvI` | M/E u8, pad, source u16 |
| cut | `DCut` | M/E u8, three zero bytes |
| auto | `DAut` | M/E u8, three zero bytes |
| transition bar | `CTPs` | M/E u8, pad, position u16 (0 to 10000) |
| transition style | `CTTp` | mask `0x01` (style), M/E, style, next selection |
| mix rate | `CTMx` | M/E u8, frames u8, two zero bytes |
| dip rate | `CTDp` | mask `0x01` (rate), M/E, frames, pad, dip source u16, two zero bytes |
| wipe rate | `CTWp` | mask u16 `0x0001` (rate), M/E, frames, then sixteen zero bytes |
| DSK on air | `CDsL` | keyer u8, on air u8, two zero bytes |
| DSK auto | `DDsA` | from protocol 2.29: mask u8 (zero), keyer u8, two zero bytes. Before 2.29: keyer u8, three zero bytes |
| DSK tie | `CDsT` | keyer u8, tie u8, two zero bytes |
| DSK rate | `CDsR` | keyer u8, frames u8, two zero bytes |
| USK on air | `CKOn` | M/E u8, keyer u8, on air u8, pad |
| fade to black | `FtbA` | M/E u8, three zero bytes |
| fade rate | `FtbC` | mask `0x01`, M/E, frames, pad |
| aux source | `CAuS` | mask `0x01`, aux u8, source u16 |
| macro run | `MAct` | index u16, action 0, pad |
| macro stop | `MAct` | `0xffff`, action 1, pad |
| macro stop recording | `MAct` | `0xffff`, action 2, pad |
| macro record | `MSRc` | index u16, name length u16, description length u16, name, description, zero padded to four bytes |
| media player still | `MPSS` | mask `0x03` (source type and still), player, source type 1, still index, four zero bytes |

Styles are numbered mix 0, dip 1, wipe 2, DVE 3, stinger 4.

## Source numbers

| Number | Source |
|---|---|
| 0 | black |
| 1 to 999 | inputs |
| 1000 | colour bars |
| 2001, 2002 | colour generators |
| 3010, 3020 | media players 1 and 2 (their keys one more) |
| 6000 | SuperSource |
| 7001, 7002 | clean feeds |
| 8001 on | aux outputs |
| 10010, 10011 | M/E 1 programme and preview (M/E n is 10000 + 10n) |

## Not read yet

Audio levels and mixer state, SuperSource, multiviewer layout, DVE and
stinger settings, key settings beyond on air, media pool contents, and
camera control. Unknown blocks are skipped, so adding one is a new entry in
`crates/godwinmix-atem/src/state/mod.rs` and a reader beside the others.
