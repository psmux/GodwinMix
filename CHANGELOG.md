# Changelog

## Unreleased

* Fixed a segfault in the programme compositor when a source that was on air was removed. A flush stop sent into a slot could free a frame the compositor's scaler threads were still writing. It took about eighty removals to hit on an M4 Pro. A slot is now hidden, and one frame let out, before its flush is ended.
* "Start empty" on the first run screen opens the Default scene's own source chooser. It used to add the source to the mixer and to no scene, so the toast said added and nothing appeared.
* The source chooser reuses a test pattern or media file the mixer already has. It compared against the full address while the core publishes a shortened one, so every scene got its own copy of colour bars.
* Undo after removing a source, and paste, no longer add a source back from its shortened address, which for a file cannot start. Where the address is not recoverable the UI says so, and asks before a removal it cannot undo.
* The uptime in the header counts on between snapshots.

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
