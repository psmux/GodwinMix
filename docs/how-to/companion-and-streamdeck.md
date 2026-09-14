# Put GodwinMix on a hardware panel

A button that takes a camera and goes red while that camera is on air. Two ways
to get one, and which you want depends on what is on your desk.

| You have | Use |
|---|---|
| Bitfocus Companion already, or any panel that is not a Stream Deck | the Companion module |
| A Stream Deck and nothing else | the Stream Deck plugin |
| Neither, but something that speaks OSC | `docs/how-to/osc-and-tally.md` |

Companion is the better answer when you have the choice. It drives Stream
Decks, X-keys, Loupedecks, stream controllers, MIDI surfaces and a web page,
and one module reaches all of them.

## Why these two are TypeScript

Everything else first party in GodwinMix is Rust. These are not, and it is
worth saying why rather than leaving it to be noticed.

Companion loads a module as a Node process and talks to it over an IPC channel
defined by `@companion-module/base`. The Stream Deck app loads a plugin as a
Node process and talks to it over Elgato's own protocol through
`@elgato/streamdeck`. There is no other way in to either host. The language is
the host's choice, not ours.

Neither is a second implementation of anything. Both are clients of
`@godwinmix/client`, which speaks the same public contract as the web UI, and
neither can do anything a script you write yourself could not.

## Companion

### Install it

```sh
cd integrations/companion
npm install
npm run build
```

In Companion's settings there is a "Developer modules path" field. Point it at
`integrations/companion` and restart Companion. Add a connection, search for
GodwinMix, and fill in two things: the mixer's address as you would type it
into a browser, and a token with the `operate` scope.

### Your first button

Go to the Presets tab, find GodwinMix, and drag "Take cam1" onto a page. Press
it. If your source is called something else, click the button and change the
source id in the action and in both feedbacks.

That preset is three things wired together, and it is worth knowing which:

* an **action**, "Take a source to programme", which calls `program.take`;
* a **feedback**, "Source is on programme", which turns the button red;
* a second feedback, "Source is on preview", which turns it green.

The feedbacks follow the mixer, not the button. Take that source from the web
UI and the Companion button still goes red.

### Everything the module gives you

**Actions**: take a source, cut to the slate, take a scene, revert, add a
source, start an output, stop an output, reconnect an output, set a fader, mute
or unmute.

There is no "start output" call in the protocol, and the action called that
sends `output.add`. The encoder is already running, so adding a destination is
the whole of starting one and it costs nothing on air.

**Feedbacks**: source on programme (red), source on preview (green), output in
a state, connected to the mixer.

**Variables**: `$(gmx:program)`, `$(gmx:program_name)`, `$(gmx:uptime)`,
`$(gmx:uptime_secs)`, `$(gmx:running_time)`, `$(gmx:source_count)`,
`$(gmx:live_source_count)`, `$(gmx:output_count)`, `$(gmx:connected)`.

Put `$(gmx:program)` in a button's text and it says what is on air.

**Presets**: four take buttons with tally, a slate, a revert, and a connection
lamp.

## Stream Deck

### Install it

```sh
cd integrations/streamdeck
npm install
npm run build
```

That writes `com.godwinmix.streamdeck.sdPlugin/bin/plugin.js`. Copy or link the
`com.godwinmix.streamdeck.sdPlugin` directory into the app's plugins folder and
restart it:

* macOS: `~/Library/Application Support/com.elgato.StreamDeck/Plugins/`
* Windows: `%APPDATA%\Elgato\StreamDeck\Plugins\`

### Your first key

Drag a Take key onto the deck. Its inspector asks for four things: the source
id, an optional title for the key, the mixer's address and a token. The address
and the token are global, so the next key you add picks them up.

Press it. The source goes on air and the key goes red.

### The three keys

| Key | A press | The colour |
|---|---|---|
| Take | puts the source on programme | red on air, green on preview, dark off, grey when the source is not a source |
| Output | starts the destination when it is stopped, stops it when it is running | green live, amber connecting, red failed, dark stopped |
| Slate | cuts the programme to black | red while the programme is black |

A key that names a source the mixer has never heard of goes grey with a question
mark rather than looking the same as a key that is simply off air. Finding a
typo during a show should not mean reading a log.

## What happens when the mixer goes away

Both stop pretending. Companion's connection goes to a failure state and the
"Connected to the mixer" feedback goes off; the Stream Deck's keys go grey. The
client underneath reconnects on its own, and when it comes back the first thing
it does is ask for a fresh snapshot, so nothing is left showing a stale colour.

## How fast the tally is

The roadmap's criterion for this phase is that a Companion button takes a source
and its feedback turns red within one frame of `event/program.took`. One frame
at 30 fps is 33 ms.

Both integrations measure it in their own test suite and print the number:

```
    feedback turned red 0.3 ms after the button was pressed
    key turned red 0.1 ms after it was pressed
```

That is against a fake core on the same machine, so it is the cost of the
integration and not of the network. The test fails over 250 ms, which is loose
on purpose: a loaded CI box is slower than a show machine, and the number is
printed on every run either way.

## Testing them without the hardware

```sh
cd integrations/companion && npm test
cd integrations/streamdeck && npm test
```

Both run with nothing installed: no registry, no lockfile, no network. They
test against a fake core that is a real HTTP server on a real port upgrading a
real socket, which is the harness from `clients/typescript/test/fake-core.ts`.

Neither test imports its host's SDK. In each package there is exactly one file
that does (`src/index.ts` for Companion, `src/plugin.ts` for Stream Deck) and it
holds no behaviour: it hands the host some tables and forwards its callbacks.
Everything else is plain functions of the mixer's state, which is what makes
this possible.

Each README says exactly what was verified that way and what still needs a
person with the real hardware.

## Reference

* `integrations/companion/README.md`, `integrations/streamdeck/README.md`
* `docs/reference/surfaces.md`: writing a whole UI, of which these are two
* `docs/how-to/control-the-mixer.md`: the calls underneath both
