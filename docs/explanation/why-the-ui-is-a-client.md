# Why the UI is a client

The mixer has no window. It is a headless process with an HTTP port, and the
web page it serves is a client of that port with no privileges the port does
not give everyone. So is `gmx ctl`. So is the MCP server an AI agent talks to.
So is the desktop app, which is a webview pointed at the same URL.

There is no faster internal path. That is a rule, not an accident of the
current code.

## The reasoning

**A reference implementation that cheats never grows an ecosystem.** If the
first party UI can do something a third party cannot, then every serious
third party client is a second class citizen, and nobody writes one. The
failure mode is visible across this software category: control surfaces that
can do 80 percent of what the application's own window can, and a community
that spends its time on the missing 20 percent.

**The operator is usually not at the machine.** A church mixer lives in a rack
in a cupboard. A channel runs on a VPS. The normal case is operating a mixer
over a network, not sitting in front of one, and a design where the local UI is
special makes the normal case the awkward one.

**The operator is increasingly not a person.** An AI director reading state and
taking sources is the same client as a human clicking cells. It gets the same
API, and it needs the state to be readable rather than rendered.

## What follows from it

* **Every feature is an API call first.** A button in the UI that has no
  corresponding route is a bug in the design, not a shortcut.
* **The desktop app is thin.** It is a webview at `http://localhost:8080` with
  two quit buttons. There is no second implementation to keep in step, and
  pointing it at a machine in another building is a change of address rather
  than a mode.
* **The UI can be replaced.** Wholly. A terminal UI, a Tkinter script, a Stream
  Deck, a Companion module and an agent are all clients of the same protocol.
  Some of those are planned and some exist; none of them requires a change to
  the core.
* **Closing the window does not stop the stream.** The stream does not live in
  the window. Closing it by accident must not take the programme down, which is
  why "Stop everything" is a separate button that asks twice.
* **The mixer must be observable over the wire.** Anything the UI needs to draw
  has to be in the state or the event stream: tally, meters, source liveness,
  output state, the mosaic. A UI cannot reach into the process for something
  that was not published.

## The cost of the rule

Two real ones, both accepted.

**Some things are harder over a socket than in a window.** Preview at low
latency is the obvious one. A local UI could share memory with the compositor;
a remote one gets MJPEG, or WHEP, or a `unixfd` preview when it happens to be
on the same host. The answer is a fallback chain, not a special case for the
first party client.

**A client can ask for expensive things.** The mosaic is an encoder. Snapshots
cost. Telemetry costs. So the second principle of the project is that nothing
runs unless a client asks for it: no multiview, no meters, no thumbnails, no
telemetry, unless something is subscribed. `[multiview] enabled = false` removes
it entirely. A mixer driven by a script with no UI attached should cost what a
mixer with no UI costs.

## How it is kept honest

The web UI's calls will be checked against the generated protocol in CI, with a
proxy recording what it asked for; a call the UI makes that is not in
`protocol.json` fails the build. That check is part of the UI split work and is
not in place yet. Until it is, the rule is held by review and by the fact that
`gmx ctl` and the MCP server are written against the same routes and would
break first.

## Further reading

* [The HTTP API](../reference/http-api.md), which is that contract today.
* [The protocol reference](../reference/protocol.md) for the generated version
  and the compatibility promise.
* [docs/agents.md](../agents.md) for the client that is not a person.
