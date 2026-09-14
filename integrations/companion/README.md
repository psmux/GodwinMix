# The GodwinMix module for Bitfocus Companion

A button on a Stream Deck, an X-keys, a Loupedeck or any of the other panels
Companion drives takes a source on the mixer, and the button goes red while
that source is on air.

Companion has over 800 modules and every hardware panel that matters already
speaks to it, which is why this exists: one module here reaches more desks than
a driver per panel ever would.

## Why this one is TypeScript

Everything first party in GodwinMix is Rust. This is not, and neither is the
Stream Deck plugin, because Companion loads a module as a Node process and
talks to it over an IPC channel defined by `@companion-module/base`. There is
no other way in. The same goes for Elgato's SDK. Both hosts require it, so both
integrations are TypeScript and the rest of the project is not.

What they are not is a second implementation of anything. Both are clients of
`@godwinmix/client`, which is a client of the same public protocol the web UI
uses. Nothing here can do anything a script you write yourself could not.

## What it gives you

**Actions**

| Action | The call it makes |
|---|---|
| Take a source to programme | `program.take` |
| Cut to the slate | `program.take` with no source |
| Take a scene | `program.take` with `scene` |
| Back to the shot before | `program.revert` |
| Add a source | `source.add` |
| Start an output | `output.add` |
| Stop an output | `output.remove` |
| Reconnect an output | `output.reconnect` |
| Set a source's fader | `source.audio.set` |
| Mute or unmute a source | `source.audio.set` |

There is no `output.start` in the protocol. The encoder is already running, so
adding a destination is the whole of starting one and it costs nothing on air.

**Feedbacks**

| Feedback | Default style |
|---|---|
| Source is on programme | red |
| Source is on preview | green |
| Output is in a state | green |
| Connected to the mixer | dark |

The tally feedbacks follow `event/tally`, which the core derives. That means a
button is right when somebody else took the source, from the web UI, the
terminal UI, an OSC surface or an agent.

**Variables**

`program`, `program_name`, `uptime`, `uptime_secs`, `running_time`,
`source_count`, `live_source_count`, `output_count`, `connected`.

**Presets**

Four take buttons with tally, a slate, a revert, and a connection lamp. Drag
one onto a page and it works.

## Install it

```sh
cd integrations/companion
npm install
npm run build
```

Then point Companion's developer modules path at this directory (Companion's
settings have a "Developer modules path" field) and add a GodwinMix connection.
Fill in the address of the mixer as you would type it into a browser, and a
token with the `operate` scope.

## Test it

```sh
npm test
```

This needs nothing installed: no registry, no lockfile, no network. The tests
run against a fake core that is a real HTTP server on a real port upgrading a
real socket, which is the harness from `clients/typescript/test/fake-core.ts`.

They do not import `@companion-module/base`. `src/index.ts` is the only file
that does, and it holds no behaviour: it hands Companion the four tables in
`src/definitions.ts` and forwards its callbacks into `src/link.ts`. Those two
files are what the tests drive.

```
ℹ tests 24
ℹ pass 24
ℹ fail 0
    feedback turned red 0.3 ms after the button was pressed
```

## What was verified here, and what was not

Companion itself cannot be installed in this repository's environment, so the
module has not been loaded by a running Companion and no physical panel has
been pressed. What has been checked, and is checked by `npm test` on every run:

* Every action makes the call it says it makes, with the params it says, over a
  real socket to a fake core: the calls are read back off the wire.
* Every feedback answers correctly for every state, including the ones that are
  easy to get wrong: a source that is not a source, a tally value the core
  never sends, a mixer that is not connected.
* Every variable has a value, including on a mixer that is showing the slate.
* Every preset names an action and a feedback that exist, which is the mistake
  that ships a module nobody can use.
* **The roadmap's acceptance criterion, measured.** "A Companion button takes a
  source and its feedback turns red within one frame of `event/program.took`."
  The test presses the button, the core answers and pushes the events, and the
  time from the press to the feedback answering `true` is recorded and printed.
  It is 0.1 to 0.5 ms on this machine, against a frame at 30 fps of 33 ms. The
  test fails over 250 ms, which is loose on purpose: a loaded CI box is slower
  than a show machine and the number is printed either way.
* A take somebody else made flips the feedback too, not just the module's own
  button.

What a person has to check on real hardware: that Companion loads the module at
all, that the config fields render, and that the presets look right on a panel.

## Publishing it

`src/gmx.ts` is the one line that imports the client by path so the tests run
with nothing installed. A published build changes it to
`export * from "@godwinmix/client";`, which `package.json` already declares.
