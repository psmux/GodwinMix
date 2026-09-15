# AGENTS.md

For a coding agent changing this plugin. Read this before touching anything.

## What this is

A GodwinMix plugin with two provides that are different sorts of thing:

* `service` `host`: a process serving each graphic placement as its own page on
  127.0.0.1, and pushing `load`, `update`, `play` and `stop` at it.
* `graphic` `lower-third`: no process and no media, an OGraf manifest and a web
  component. A plugin that ships only graphics needs no `[run]` at all.

It runs as a sidecar the core starts, and by hand with `--serve` so a template
author can look at a graphic in a browser. `main.rs` chooses on
`PluginEnv::started_by_core()`.

## Build and test

```sh
cargo test -p gmx-ograf                      # 26 unit tests
./check                                      # fmt, clippy, tests, every graphic well formed
dev/plugins.sh build
gmx plugin test --offline plugins/ograf
gmx plugin test plugins/ograf
```

## Where things are

| File | What is in it |
|---|---|
| `src/catalogue.rs` | finding a graphic on disk, through the SDK's manifest parser |
| `src/state.rs` | what each placement is showing, and the channel the pages read |
| `src/host.rs` | the HTTP server: seven routes, no framework |
| `src/page.rs` | the wrapper page, the only HTML this plugin writes |
| `src/main.rs` | the process, the `graphic` tool, the standalone preview |
| `examples/lower-third/` | the example graphic: copy this to start one |

## Rules this plugin holds to

1. **The state is here, not in the page.** A browser source is a process the
   supervisor may restart. If the words lived in the page, a restart would put
   a blank strap on air. Every route and every action goes through
   `state::Host` and the page fetches `/state/<instance>` on connect.
2. **One page per placement.** A template that throws takes its own page down
   and not the other three. Never serve two graphics from one page.
3. **Loopback only.** `host::serve` binds `Ipv4Addr::LOCALHOST`. Do not make
   that configurable: the only client is a browser on this machine, and a
   graphics host on the network is a way to put words on somebody's programme.
4. **Nothing paints.** The wrapper page's background is transparent. What is
   behind a graphic is the mixer's business.
5. **Names off a URL are checked, never joined.** `catalogue::is_name` and
   `catalogue::asset` are the two places that do it, and both are tested with
   a path that climbs out.
6. **No web framework.** Seven routes over a `TcpListener`. A sidecar that runs
   on a Raspberry Pi does not carry a router and a TLS backend to serve a page.

## When you add a graphic

Put it under `examples/<name>/` with a `graphic.ograf.json` and the module it
names, add a `[[provides]] kind = "graphic"` block to `gmx-plugin.toml`, and
`./check` will hold you to the four OGraf methods. Nothing else has to change:
the catalogue reads the manifest.

## What is deliberately not here

* Alpha. The mixer graph is I420. See `docs/reference/graphics.md`.
* A renderer of its own. The page is rendered by `browser/source`, which is the
  CEF sidecar or `wpesrc`. This plugin never touches GStreamer.
* Any knowledge of scenes. The core resolves a `content.graphic` item to a
  source and calls this plugin; this plugin does not read a scene document.
