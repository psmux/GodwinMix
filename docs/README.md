# GodwinMix documentation

Four kinds of page, kept apart on purpose. A tutorial takes you from nothing to
a working thing and never stops to explain. A how to guide answers one
question you already have. Reference tells you what a thing is. Explanation
says why it is like that. Mixing them makes a page that serves nobody, which is
the shape most project documentation ends up in.

Where a feature does not exist yet, the page says so in a sentence and names
the command that will do it. Nothing here shows output from a command that
cannot be run today.

Engineers and operators who want everything on one page, the install paths,
what has been verified, the pipeline and the platforms, start at
[Technical details](technical-details.md).

## Tutorials

Start here if you have never run it.

* [Your first stream in five minutes, with Docker](tutorials/first-stream-docker.md)
* [Your first stream with the desktop app](tutorials/first-stream-desktop.md)
* [Your first plugin](tutorials/your-first-plugin.md)
* [Your first stream with a preset](tutorials/first-stream-with-a-preset.md), the
  church path from install to on air
* [Your first plugin](tutorials/your-first-plugin.md) (planned; the page says what is missing)
* [Your first stream, from the web page](tutorials/first-stream-web.md)


## How to

* [Install on Windows](how-to/install-on-windows.md),
  [macOS](how-to/install-on-macos.md),
  [Linux](how-to/install-on-linux.md)
* [Start the mixer for the first time](how-to/first-run.md)
* [Add a source](how-to/add-a-source.md): cameras, screens, microphones, files, pages and feeds
* [Run it on a headless server](how-to/headless-server.md)
* [Restart the mixer from the page](how-to/restart-the-mixer.md)
* [Serve the control port over HTTPS](how-to/serve-https.md)
* [Put it behind a reverse proxy with TLS](how-to/reverse-proxy.md)
* [Run it on a Raspberry Pi](how-to/raspberry-pi.md)
* [Choose a hardware encoder](how-to/choose-a-hardware-encoder.md)
* [Put more than one thing on screen](how-to/scenes.md)
* [Change scene with a transition](how-to/transitions.md)
* [Use transitions and effects from packs: light leaks, stingers, luma wipes, shaders](how-to/transitions-from-packs.md)
* [Build a scene by dragging](how-to/compose-a-scene.md)
* [Operate one mixer with several people](how-to/operate-with-several-people.md),
  from phones and desks at once, each with their own undo
* [Make your plugin editable in every designer](how-to/extend-the-designer.md)
* [Roll an ad break](how-to/ad-breaks.md)
* [See and hear the mixer from anywhere](how-to/preview-and-audio.md)
* [Run the terminal UI](how-to/terminal-ui.md)
* [Put GodwinMix on a hardware panel](how-to/companion-and-streamdeck.md),
  Bitfocus Companion and Elgato Stream Deck
* [Control the mixer from OSC, and light the tally lamps](how-to/osc-and-tally.md)
* [Let the mixer cut by itself](how-to/run-a-director.md), rule based first,
  then with a model and what that costs
* [Add a theme](how-to/add-a-theme.md) (planned)
* [Write a preset](how-to/write-a-preset.md) (planned)
* [Add a theme](how-to/add-a-theme.md)
* [Make a preset](how-to/make-a-preset.md)
* [Make a custom build](how-to/custom-build.md)
* [Sign the Windows builds](how-to/sign-windows-builds.md) with the Certum cloud certificate, and test it on a tag that publishes nothing
* [Upgrade from LiveboxMix](how-to/upgrade-from-liveboxmix.md)
* [Write a UI of your own](how-to/write-a-ui.md)
* [Run the smoke test](how-to/smoke-test.md)
* [Run the soak test](how-to/soak-test.md), the same things over and over for
  an hour, watching memory, descriptors and threads
* [Embed the engine in your own program](how-to/embed-the-engine.md)
* [Write a WASM plugin: a sandboxed service or transition, no media](how-to/write-a-wasm-plugin.md)
* [Write a source plugin in Rust](how-to/write-a-source-plugin.md)
* [Install a plugin](how-to/install-a-plugin.md)
* [Add a second machine](how-to/add-a-node.md)
* [Test a plugin](how-to/test-a-plugin.md)
* [Publish a plugin](how-to/publish-a-plugin.md)
* [Run a marketplace](how-to/run-a-marketplace.md)
* [Receive a phone or an OBS stream](how-to/receive-a-phone-or-obs-stream.md)
* [Stream to YouTube, Facebook or Twitch](how-to/stream-to-a-platform.md)
* [Receive and send SRT](how-to/srt.md)
* [Send the programme to a WHIP endpoint](how-to/send-to-whip.md)
* [Use NDI](how-to/use-ndi.md)
* [Use a webcam](how-to/use-a-webcam.md)
* [Capture the screen](how-to/capture-the-screen.md)
* [Share a camera between shows](how-to/share-a-camera-between-shows.md)
* [Run several shows on one machine](how-to/run-several-shows.md)
* [See where every input goes, and send it somewhere else](how-to/route-inputs-to-outputs.md)
* [Record to a file](how-to/record-to-a-file.md)
* [Serve HLS to viewers](how-to/serve-hls.md)
* [Watch the programme over WebRTC](how-to/watch-over-webrtc.md)
* [Serve the programme over RTSP](how-to/serve-rtsp.md)
* [Send and receive RIST](how-to/rist.md)
* [Show a picture, or a run of pictures](how-to/show-a-picture.md)
* [Add text and a ticker](how-to/add-text-and-a-ticker.md)
* [Show live data on air: RSS, JSON, a sheet or a websocket](how-to/show-live-data.md)
* [Put a presenter on a green screen into a designed studio](how-to/green-screen-presenter.md)
* [Replace the background behind a person, with or without a green screen](how-to/replace-the-background.md)
* [Add an IP camera](how-to/add-an-ip-camera.md)
* [Internet radio: send the sound out, or play a station in](how-to/radio.md)
* [Capture SDI or HDMI from a DeckLink card](how-to/capture-sdi.md)
* [Capture SDI or HDMI from a DeckLink card](how-to/capture-sdi.md)
* [Run your own code when something happens](how-to/hooks.md)
* [Turn a bug into a test](how-to/turn-a-bug-into-a-test.md)
* [Add a built in source kind](how-to/add-a-built-in-source-kind.md)
* [How to add or override a codec entry](how-to/add-a-codec-entry.md)
* [Control the mixer](how-to/control-the-mixer.md)
* [Change a setting](how-to/change-a-setting.md)
* [Debug a show](how-to/debug-a-show.md)
* [The desktop app](how-to/desktop-app.md)
* [Stop streaming, and quit without leaving anything running](how-to/stop-streaming-and-quit.md)
* [Run a show from phones](how-to/run-a-show-from-phones.md): let the network in, scan a code, take shots from a phone
* [Import your scenes from OBS](how-to/import-from-obs.md)
* [Make a graphic](how-to/make-a-graphic.md)
* [Write an SVG template](how-to/write-an-svg-template.md)
* [Connect an AI agent, and have it make your graphics](how-to/connect-an-ai-agent.md)
* [Design graphics with an AI agent](how-to/design-graphics-with-ai.md)
* [Build your own graphics gallery with an AI agent](how-to/build-a-graphics-gallery-with-ai.md): any agent saves lower thirds, backgrounds, tickers, bugs and sets, you put them on air in two clicks
* [Make a graphic that moves](how-to/make-a-moving-graphic.md): an HTML template with fields and its own way in and out, transparent over the programme
* [Use the mixer controls](how-to/preview-monitor.md)
* [Run on a Raspberry Pi](how-to/run-on-a-raspberry-pi.md)
* [Share a collection](how-to/share-a-collection.md)
* [Use the mixer from an AI agent](how-to/use-with-an-ai-agent.md)
* [Write a panel](how-to/write-a-panel.md)
* [Add shows in bulk](how-to/add-shows-in-bulk.md)
* [Benchmark GodwinMix at headend scale](how-to/benchmark-at-scale.md)
* [Take streams from several encoders into one channel](how-to/channels.md)
* [Move your channels from Livebox](how-to/move-from-livebox.md): keep the address and password your encoders already send
* [Customize the workspace](how-to/customize-the-workspace.md)
* [Run the mixer from a phone or a tablet](how-to/use-a-phone-or-tablet.md): long press for the item menu, double tap to open, drag by the grip
* [Install the control page as an app](how-to/install-as-an-app.md): on a phone or a desktop, and the certificate a phone needs for it
* [Take headend feeds into direct shows](how-to/headend-feeds.md)
* [Watch many shows at once](how-to/monitor-many-shows.md)
* [Send a channel on to YouTube, Facebook or Twitch](how-to/restream-a-channel.md)
* [Record a channel and share a watch link](how-to/record-a-channel.md)
* [Save and open a project](how-to/save-and-open-a-project.md)
* [Send in several formats](how-to/send-in-several-formats.md)
* [Receive and send MPEG-TS over UDP and multicast](how-to/udp-and-multicast.md)
* [Upload a local media file](how-to/upload-local-media.md)
* [Use this browser's camera and microphone](how-to/use-this-browsers-camera.md)
* [Add a phone's camera with one scan](how-to/use-this-browsers-camera.md#add-a-phones-camera)


## Reference

* [The HTTP API](reference/http-api.md)
* [Safety: the minimum hold, the rate limit, the flash guard](reference/safety.md)
* [`agent.state`, both formats and their sizes](reference/agent-state.md)
* [Tasks: work that outlives the call that started it](reference/tasks.md)
* [The command line](reference/cli.md)
* [The keyboard](reference/keyboard.md)
* [Configuration, every key](reference/configuration.md)
* [The config methods: config.get, config.set, config.reset, config.schema](reference/config-methods.md)
* [Device tokens: token.create, token.list, token.revoke](reference/device-tokens.md)
* [Letting other devices reach the mixer: network.share](reference/network-share.md)
* [Presets, every manifest key and the merge rules](reference/presets.md)
* [Scene commands, every method with an example](reference/scene-commands.md)
* [Transitions: the built in types, the plugin contract, the accuracy](reference/transitions.md)
* [Transitions and effects from packs: the formats, the folder, the fx methods](reference/fx.md)
* [The scene document](reference/scene-document.md)
* [The designer kits](reference/designer-kits.md)
* [Presence: client ids, presence.list and presence.changed](reference/presence.md)
* [Sources](reference/sources.md)
* [Web page sources](reference/web-page-sources.md)
* [Text, ticker and transparent sources](reference/text-sources.md)
* [Live data: the feed methods, events, formats and limits](reference/live-data.md)
* [Feed paths and templates](reference/feed-paths.md)
* [The chroma key](reference/chroma-key.md)
* [The cutout: a person with no screen behind them](reference/cutout.md)
* [Streams: MJPEG, PCM, Opus, WHEP and the local socket](reference/streams.md)
* [The generated protocol reference](reference/protocol.md)
* [The first party plugins](reference/plugins.md)
* [The plugin manifest](reference/plugin-manifest.md)
* [The frame bus](reference/frame-bus.md)
* [The index format](reference/index-format.md)
* [The quality scale](reference/quality-scale.md)
* [Surfaces: a whole UI, and `gmx ui`](reference/surfaces.md)
* [The plugin protocol](reference/plugin-protocol.md)
* [The plugin lifecycle](reference/plugin-lifecycle.md)
* [Plugins as WebAssembly components (tier W)](reference/wasm.md)
* [Nodes](reference/nodes.md)
* [The network plugins: srt, whip, ingest and ndi](reference/plugins-network.md)
* [The client libraries](reference/clients.md)
* [Errors and the button they offer: `data.action`](reference/errors.md)
* [Hooks](reference/hooks.md)
* [The session log](reference/session-log.md)
* [Reference: the shipped codec catalogue](reference/codecs.md)
* [Every way in and every way out: transports, containers, codecs](reference/formats.md)
* [Renditions and the planner: the rules, the plan, the reasons](reference/renditions.md)
* [The HLS output: params, routes, the viewer key](reference/hls-output.md)
* [Graphics](reference/graphics.md)
* [Graphic templates: the SVG kind, the pack, the methods](reference/graphic-templates.md)
* [The gallery methods](reference/gallery.md): `gallery.*`, their tools and their byte routes
* [Graphics for agents](reference/graphics-for-agents.md): what a model must produce for each kind of graphic, with an example of each
* [Agent setup](reference/agent-setup.md): `agent.tools`, `agent.setup` and `godwinmix agent setup`, the files each tool's setup writes
* [The gallery's item format](reference/gallery-format.md): one folder and a `graphic.toml` per graphic, the kinds, the zip
* [Metrics](reference/metrics.md)
* [Preview monitor status](reference/preview-monitor.md)
* [The channel methods](reference/channels.md)
* [Reference: what a direct show takes in](reference/direct-inputs.md)
* [Direct shows: the host's table, events and calls](reference/direct-shows.md)
* [Reference: the resource governor](reference/governor.md)
* [Media upload](reference/media-upload.md)
* [Project files and the project methods](reference/project.md)
* [The publisher page](reference/publisher-page.md): `/join/`, its links, the stream name, VP8 and what a phone gets
* [Recording outputs](reference/recording.md)
* [Setup on first use](reference/setup.md)
* [Show health: alarms, thresholds and thumbnails](reference/show-health.md)
* [Reference: shows and the station](reference/shows.md)
* [Browser file sources](reference/source-file-picker.md)
* [The udp plugin](reference/udp.md)
* [Workspace layout](reference/workspace-layout.md)


## Explanation

* [Why the programme never stops](explanation/why-the-programme-never-stops.md)
* [Nothing runs unless asked](explanation/nothing-runs-unless-asked.md)
* [Why the planner copies first and shares every encoder](explanation/rendition-planner.md)
* [How the HLS output cuts, keeps and serves segments, and what it costs](explanation/hls-output.md)
* [Why plugins are processes](explanation/why-plugins-are-processes.md)
* [Why WASM is not on the frame path](explanation/why-wasm-is-not-on-the-frame-path.md)
* [The frame bus: decode once, read everywhere](explanation/frame-bus.md)
* [One plugin, three placements](explanation/one-plugin-three-placements.md)
* [Why the UI is a client](explanation/why-the-ui-is-a-client.md)
* [How a scene reaches the compositor](explanation/how-a-scene-reaches-the-compositor.md)
* [How late the picture is: every stage from camera to page](explanation/how-late-the-picture-is.md)
* [Undo when several people edit one show](explanation/undo-with-several-people.md)
* [Footprint budgets and the reference machines](explanation/footprint-budgets.md)
* [The crate map](explanation/architecture.md)
* [Cross platform: what is gated where, and why](explanation/cross-platform.md)
* [Trust and signing](explanation/trust-and-signing.md)
* [Why the evals grade the world](explanation/evals.md)
* [Footprint](explanation/footprint.md)
* [How a source works](explanation/how-a-source-works.md)
* [Converting a channel's stream for one destination](explanation/channel-transcoding.md)
* [The direct host](explanation/direct-host.md)
* [The resource governor](explanation/resource-governor.md)


## For AI agents

* [The operator playbook](agents.md)
* [The friction log](friction-log.md), which is where a stuck moment on the
  developer path gets written down whether the person stuck was human or not.
