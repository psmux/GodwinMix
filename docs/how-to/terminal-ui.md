# Run the terminal UI

`gmx-tui` is a mixer panel in a terminal. It was built for the case the web UI
cannot cover: the mixer is on a server in a rack or in a datacentre, you are on
the end of an SSH session, and you want tally, meters and a take key without
forwarding a port or opening a browser.

It holds no private channel to the core. It opens one WebSocket to `/rpc`,
subscribes the way `docs/how-to/control-the-mixer.md` describes, and every key
sends a JSON-RPC method any other client could send. Nothing it does is
unavailable to a script you write yourself.

## Build it

The crate is `crates/godwinmix-tui`, one member of the workspace:

```sh
cargo build --release -p godwinmix-tui
./target/release/gmx-tui --help
```

There is no GStreamer in it and it links nothing the core links, so this
builds on a machine with no media stack at all. The binary is 2.8 MB on macOS
arm64, 2.4 MB once stripped, and it needs nothing where it runs but a
terminal. That is the point: copy it to a jump host and leave the mixer where
it is.

## Connect

```sh
gmx-tui --url http://mixer.local:8080 --token a-long-random-string
```

`--url` takes what you would type at a browser. `http` becomes `ws`, `https`
becomes `wss`, and `/rpc` is added when the URL has no path of its own, so
`--url mixer.local:8080` works too. The environment variables are the ones
`gmx ctl` already uses, so if they are exported you can run `gmx-tui` with no
arguments at all:

```sh
export GODWINMIX_URL=http://mixer.local:8080
export GODWINMIX_TOKEN=a-long-random-string
gmx-tui
```

Or let `gmx` start it for you. The TUI is registered as a `surface` plugin, so:

```sh
gmx ui tui
```

finds the binary, passes the address and the token it is already using, and
hands over the terminal. `gmx ui list` shows every surface installed, and
`gmx ui tui -- --multiview` passes flags through. See
[surfaces](../reference/surfaces.md).

The token goes in an `Authorization: Bearer` header, not in the URL, because a
token in a URL ends up in more logs than it should. A token with the `operate`
scope can run a show; a `read` token shows everything and is refused on the
keys that change something, with the refusal on the footer.

## The screen

This is a real session against a core with four test sources and no
destination, a moment after `2` put the bouncing ball on air:

```
┌ programme ───────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│ON AIR   Bouncing ball (ball)                                                                                         │
│running 00:01:05  up 00:01:05  vtenc_h264_hw / avenc_aac (hardware)                                                   │
└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘
┌ sources (4) ───────────────────────────────────────────────────┐┌ destinations (0) ──────────────────────────────────┐
│1 ● Colour bars               live           ████████      +0 dB││no destinations. The programme is being mixed and go│
│2 ● Bouncing ball             live       PGM ────────      +0 dB│└────────────────────────────────────────────────────┘
│3 ● Colour bars               live           ────────      +0 dB│┌ alerts (0, newest first) ──────────────────────────┐
│4 ● Bouncing ball             live           ────────      +0 dB││nothing has gone wrong yet.                         │
│                                                                ││                                                    │
└────────────────────────────────────────────────────────────────┘└────────────────────────────────────────────────────┘
on air: ball
```

Four panes and a log line.

**programme** is what is on air, the programme pipeline's running time, how
long the mixer has been up, and the encoder pair it picked. An ad break adds a
line here while one is armed or rolling. The third line, cut from the sample
above, says whether the link is there: `linked` with the sequence number and
the programme peak, `connecting`, or `no link` with the reason and the seconds
until the next attempt.

**sources** is one row each: the slot number the take keys use, a colour dot,
the name, the state (`connecting`, `live`, `stalled`, `failed`), the tally
(`PGM` on air, `PVW` on preview), a peak meter, the fader in decibels, and for
a clip its position and duration. The colour comes from the source id until the
core grows a colour of its own, so a source keeps the same dot between runs and
between clients.

**destinations** is each output with its state, how many times it has
reconnected and how many seconds are queued in front of it. A queue that climbs
and stays up is the destination failing to keep up.

**alerts** is the last 50 `event/alert` messages, newest first, with the time
in UTC. Refusals and link drops are written here too, so you can look back at
what happened during a show rather than at whatever the footer says now.

The **log line** at the bottom is the last thing that happened: the call you
just made, or the refusal that came back. A refusal is shown whole, because the
core writes its messages to be read: they name the current state and the next
step.

## The keys

`?` shows them while it runs, and `docs/reference/keyboard.md` is the full
table with the method each one calls. The short version: `1` to `9` take the
nth source on screen, `0` cuts to black, `r` reverts, `m` mutes the selected
source, `+` and `-` move its fader a decibel at a time, `a` starts or ends an
ad break, `o` goes to the destinations, `/` filters, `Tab` moves between panes,
`q` quits.

The filter decides what the number keys count. With `/clip` typed and
accepted, `1` takes the first source whose id or name matches, not the first
source the mixer has. That is deliberate: the keys count what you can see.

## The picture, and what it costs

Without `--multiview` the core is never asked for a mosaic. It builds no
mosaic pipeline for this client, `gmx_multiview_subscribers` on `/metrics`
stays at zero, and no binary frame is ever sent. That is the default because a
terminal UI over a slow link is often exactly the case where you cannot afford
the pictures.

With it:

```sh
gmx-tui --multiview                      # 4 fps, 320 pixels wide
gmx-tui --multiview --fps 8 --width 640  # more, if the link can take it
```

The mosaic box appears above the destinations and its title says what it is
costing, for example `mosaic 320x180 at 4 fps, blocks (44 frames)`.

What it costs, measured against a core on the same machine at the defaults:

| Cost | Where |
|---|---|
| the mosaic pipeline in the core | built on the first subscriber, taken down after the last one leaves. It is one compositor and one JPEG encoder at the size you asked for. |
| about 40 to 80 KB a second on the link | 4 frames a second of JPEG at 320 pixels wide. At `--fps 8 --width 640` expect four to six times that. |
| one JPEG decode per frame in the terminal | 4 a second at 320 pixels is nothing on a laptop and noticeable on a Pi. |

Three ways of drawing it, in order of preference, detected rather than assumed:

| How | When | What it looks like |
|---|---|---|
| kitty graphics | the terminal answers the kitty graphics query, or says it is kitty, Ghostty or WezTerm | the actual picture |
| sixel | the terminal's primary device attributes include attribute 4 | the actual picture, 216 colours |
| half blocks | everything else | two pixel rows per character cell, coarse but always right |

The query is sent once at startup and waits 400 ms for an answer. On Windows
the query is not sent (a Windows console does not hand raw replies back the way
a Unix tty does) and the choice comes from the environment, so pass
`--picture kitty`, `--picture sixel` or `--picture blocks` there if you know
better than the default. `--picture` works everywhere and skips detection.

## Over SSH, which is the point

```sh
ssh -t operator@mixer.local /usr/local/bin/gmx-tui --token "$GODWINMIX_TOKEN"
```

`-t` asks for a terminal, which a TUI needs. Running the binary on the mixer
itself means the WebSocket never leaves the machine and only the terminal
traffic crosses the network, which is a few KB a second without `--multiview`.

If you would rather run the binary on your own laptop and reach the mixer
through SSH, forward the control port instead:

```sh
ssh -N -L 8080:127.0.0.1:8080 operator@mixer.local &
gmx-tui --url http://127.0.0.1:8080 --token "$GODWINMIX_TOKEN"
```

Either way, put it in `tmux` or `screen` on the server if you want the panel to
survive your connection dropping:

```sh
ssh -t operator@mixer.local tmux new -As mix gmx-tui
```

The link looks after itself. When the socket drops, the screen says `no link`
with the reason and counts down to the next attempt (half a second, doubling to
fifteen), the numbers are cleared rather than left to look live, and when it
comes back it subscribes again and repaints from a fresh snapshot. A client
that falls behind is told so by the core with `event/resync`, and that is
handled the same way, without dropping the connection.

## What it does not do

* There is no `output.start` or `output.stop` in api_level 1: a destination
  either exists or it does not. `s` on a destination therefore calls
  `output.reconnect`, which drops the connection and makes it again. Adding and
  removing destinations is `gmx ctl output add` and the web UI.
* Nothing here creates sources, scenes or filters. Add a source with the web
  UI, `gmx ctl source add`, or curl, and it appears on the list within a frame.
* `r` needs `program.revert`. A core that does not have it answers `-32601`,
  and after the first refusal the key says so instead of asking again.
* There is no audio monitoring. Meters tell you there is sound; hearing it
  needs `/pcm/` or WHEP, which this build of the core does not serve yet.

## When something is wrong

| What you see | What it means |
|---|---|
| `no link  <reason>. Trying again in 2 s` | the socket went away. The reason is the one the operating system or the server gave. Nothing on the screen below it is live. |
| `connecting  waiting for the snapshot` | the socket is open and `core.subscribe` has not been answered yet. A busy core can take a few seconds. |
| `this core ignored ext: thumb, preview` | the core accepted the subscription but does not implement those streams. Everything else is running. This build asks only for meters, tally and positions, so it should be empty. |
| `the link to the mixer dropped (HTTP error: 401 Unauthorized)` | the mixer has a token and you did not pass one, or passed the wrong one. |
| `the link to the mixer dropped (IO error: Connection refused ...)` | nothing is listening there. Check `--url` and that the core is up. |
| `sources (waiting for the mixer)` that never fills | the subscription was answered but no snapshot arrived. Check the core's log. |

## Where to read more

* `docs/reference/keyboard.md`: every key, and the method it calls.
* `docs/how-to/control-the-mixer.md`: the protocol this is a client of.
* `protocol.md` at the repository root: every method, event and type.
