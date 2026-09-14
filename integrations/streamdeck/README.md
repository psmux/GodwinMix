# The GodwinMix plugin for Elgato Stream Deck

Three keys: a take key that goes red when its source is on air, an output
toggle, and a slate key.

If you already run Bitfocus Companion, use the module in
`integrations/companion` instead. It reaches every panel Companion drives, this
one reaches a Stream Deck. This exists for the operator who has a Stream Deck
and nothing else, which is a lot of people.

## Why this one is TypeScript

Everything first party in GodwinMix is Rust. This is not, because the Stream
Deck app loads a plugin as a Node process and talks to it over Elgato's own
protocol through `@elgato/streamdeck`. There is no other way in. The Companion
module is TypeScript for the same reason. Both hosts require it.

Neither is a second implementation of anything: both are clients of
`@godwinmix/client`, which speaks the same public protocol as the web UI.

## The keys

| Action | What a press does | What the colour says |
|---|---|---|
| Take | `program.take` with the key's source | red on air, green on preview, dark off, grey when the source is not a source |
| Output | adds the destination when it is stopped, removes it when it is running | green live, amber connecting or reconnecting, red failed, dark stopped |
| Slate | `program.take` with no source | red while the programme is black |

Red for on air and green for preview are the broadcast convention. A panel that
uses anything else will be misread under pressure.

The Stream Deck SDK has no colour API: a key is a title over an image. Each key
draws itself as a small SVG data URI, a few hundred bytes, which the device
rescales for whichever model is plugged in.

## Install it

```sh
cd integrations/streamdeck
npm install
npm run build
```

That writes `com.godwinmix.streamdeck.sdPlugin/bin/plugin.js`. Copy or symlink
`com.godwinmix.streamdeck.sdPlugin` into the Stream Deck app's plugins
directory and restart the app:

* macOS: `~/Library/Application Support/com.elgato.StreamDeck/Plugins/`
* Windows: `%APPDATA%\Elgato\StreamDeck\Plugins\`

Drag a Take key onto the deck, and in its inspector fill in the source id, the
mixer's address and a token with the `operate` scope. The address and the token
are global settings, so the other keys pick them up.

## Test it

```sh
npm test
```

Nothing installed, no registry, no network. The tests run against a fake core
that is a real HTTP server on a real port upgrading a real socket, which is the
harness from `clients/typescript/test/fake-core.ts`.

They do not import `@elgato/streamdeck`. `src/plugin.ts` is the only file that
does, and it holds no behaviour: it turns the SDK's callbacks into the
functions in `src/keys.ts` and the calls in `src/link.ts`, which is what the
tests drive.

```
ℹ tests 19
ℹ pass 19
ℹ fail 0
    key turned red 0.1 ms after it was pressed
```

## What was verified here, and what was not

The Stream Deck app cannot be installed in this repository's environment, so
the plugin has not been loaded by a running Stream Deck and no physical key has
been pressed. What has been checked, and is checked by `npm test` on every run:

* Every key's colour, for every state, including the ones that are easy to get
  wrong: a key naming a source the mixer has never heard of, a key with nothing
  set on it, and a mixer that is not there.
* Every press makes the call it says it makes, read back off the wire: a take,
  an output toggle in both directions, a slate.
* A take of a source that is already on air sends nothing.
* An output key with no address says so rather than failing silently.
* The tile is a valid SVG data URI, it carries the colour and the title, it
  escapes a name with an ampersand in it, and a two line title stays on the key.
* The time from a press to the key's colour changing after `event/program.took`
  is recorded and printed. It is 0.1 to 0.5 ms on this machine.
* `manifest.json` declares exactly the three actions the plugin registers, and
  every UUID is inside the plugin's namespace, which is the mistake that makes
  the Stream Deck app refuse a plugin with no useful message.

What a person has to check on real hardware: that the app loads the plugin, that
the property inspectors render, and that the SVG tiles look right on a key. The
manifest has no `imgs/` icons in it yet; the app falls back to a default icon,
and adding them is a designer's job, not a test's.

## Publishing it

`src/gmx.ts` is the one line that imports the client by path so the tests run
with nothing installed. A published build changes it to
`export * from "@godwinmix/client";`, which `package.json` already declares.
