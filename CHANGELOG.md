# Changelog

## 0.2.0 (2026-09-15)

The first GodwinMix release, grown from LiveboxMix 0.1.0.

* The core is a library (`godwinmix-core`) with a versioned protocol (`godwinmix-protocol`, api level 1) served over `/rpc` and `/api/v1`, with generated TypeScript, Python and Rust clients.
* Plugins are processes on a documented contract: a manifest, JSON lines on stdio, media over unixfd, shm or a pipe. Twelve first party plugins, an SDK, templates in five languages, a conformance harness, marketplaces with signed installs, and a WASM tier behind a feature.
* Scenes as a document the core owns, a slot pool on the compositor, transitions on control bindings, a composer in the web UI, layouts, presets, themes, an OBS importer and OGraf graphics.
* Nothing runs unless asked: the multiview, thumbnails, preview, audio monitoring and the encoder start with the first client and stop with the last. Idle core measured at 0.012 cores and 37 MB on an M4 Pro.
* Safety in the core, a compact agent surface with MCP over stdio and HTTP, hooks, session replay and an operator eval suite.
* A terminal UI, Companion, Stream Deck, OSC and tally integrations, a node daemon for remote placements over mTLS.
* Desktop app for macOS, Windows and Linux with a trimmed GStreamer bundled; Docker image and a headless quickstart.

Known limits: Linux and Windows builds are verified by inspection and cross checks only until the project's CI runs; browser sources on macOS and Windows need the codec enabled CEF the project publishes; the second canvas is not built.
