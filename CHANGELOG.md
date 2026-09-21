# Changelog

## Unreleased

* Fixed a segfault in the programme compositor when a source that was on air was removed. A flush stop sent into a slot could free a frame the compositor's scaler threads were still writing. It took about eighty removals to hit on an M4 Pro. A slot is now hidden, and one frame let out, before its flush is ended.
* "Start empty" on the first run screen opens the Default scene's own source chooser. It used to add the source to the mixer and to no scene, so the toast said added and nothing appeared.
* The source chooser reuses a test pattern or media file the mixer already has. It compared against the full address while the core publishes a shortened one, so every scene got its own copy of colour bars.
* Two new methods. `source.duplicate` copies a source the mixer has, and `source.restore` puts back one of the last sixteen it removed, with its id, fader and mute. Undo after removing a source, and paste, use them. They used to send back the shortened address the core publishes, which for a file is `file:///…` and cannot start.
* A source restarting in place no longer sends its flush into the compositor. The flush stops at the first queue past the proxy, which is all it has to wake, so the same segfault cannot be reached that way either, and the last frame stays up until the first new one.
* A plugin's media sockets stay inside the 104 byte limit on a Unix socket address. Past it the address was cut with nothing said and the socket was made outside its own directory, where nothing removed it, so the source failed with "Address already in use" on every later start, across restarts of the mixer. A desktop install put `macbook-pro-camera` at 101 bytes. Long addresses go under the temporary directory, and what a killed mixer left behind is cleared before the next bind.
* The scene chooser's Existing sources list has Remove, which takes a source out of the mixer and closes its camera, with Undo. Delete on a tile only ever meant out of this scene, so a camera once added stayed open until the mixer stopped. The chooser opens on that list when the mixer has a source the scene lacks.
* The composer shows the draft it is editing, live. `scene.preview.set` takes a `draft`, and the preview compositor lays it out through the programme's own placements, so an item dragged in the composer moves its video and not only its outline. It used to show the armed scene or the programme, neither of which is the draft. The picture is also fitted to the canvas the outlines use, where it had been a few percent off, and the title carries the scene's name where it carried an id.
* Apply in the composer reaches the programme when the scene is on air. It used to save the scene and leave the old layout going out until somebody took the scene again.
* A scene or source tile keeps its size when it goes on air. The programme monitor's `.program` rule, a centred grid, also matched a tile in the `program` state and shrank its face to 9 by 22 px.
* Every scene has a pencil, beside its tab and on its tile, that opens the composer.
* The Outputs panel's Edit, Reconnect, Remove and Stop recording can be pressed. Every status made each row again, about thirty times a second with a destination reconnecting, so a button held for a tenth of a second was gone before the release. Rows are now kept and written into.
* A custom RTMP destination takes a whole address with the key box empty, on an add and on an edit, which is what its own description promised.
* A mixer started with a relative config path, `--config godwinmix.toml`, can start a camera. The runtime directory was relative with it, and a plugin, which runs somewhere else, could not bind a socket at the address it was given.
* A camera or microphone that is not on the machine is refused with the ones that are. The plugins fell back to the bare capture element whenever the device monitor did not find what was asked for, and that fallback answered with advice to clear an `element` setting nobody had set. The by hand form's Camera box is now a choice of the devices `device.discover` found, by name, so a name typed where an id belongs cannot be sent.
* The capture plugins wait a moment for the bus before reporting a failed start, so the message carries GStreamer's reason.
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
