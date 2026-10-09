# Stop streaming, and quit without leaving anything running

Everything that goes out of the mixer can be stopped from where you are
looking, and nothing keeps running after you close the desktop app unless
you said it should.

## See what is running

The pill near the left of the header counts what is going out: "1 destination
live", "No destinations", "Streaming stopped". While anything is live it is
red and carries a clock, the longest anything has been live without a break.
It is a button. Press it, on a desk or on a phone, and **What is running**
opens, with one row for each of these:

* a destination the programme is sent to (YouTube, Facebook, a server of your
  own), with how long it has been live and its bit rate;
* a recording, with how long it has run;
* a watch link this mixer serves;
* a channel sending a stream on to a platform, named by channel;
* a channel receiving from an encoder.

Each row has its own Stop. A channel receiving from an encoder has Turn away
instead, which switches the channel off so the encoder is disconnected and
refused until you switch it on again under Channels.

**Stop all streaming** at the bottom stops every stream, watch link and
recording on the list. It does not turn encoders away. When a recording is on
the list the button says **Stop all streaming and recording**.

The panel reads the mixer every two seconds while it is open, for the bit
rates, and stops reading when you close it.

## Stop one stream

Press Stop on its row in What is running, or **Stop streaming** on its row
under Outputs. When it is live you are asked once:

> Stop streaming to YouTube? Viewers see the stream end.

**Stop streaming** stops it. **Keep it running** leaves it alone. A destination
that is still connecting is stopped without the question, since nobody is
watching it yet.

Stopping keeps the destination, its address and its stream key. Its row under
Outputs says "Stopped", with "Nothing is sent. The address and stream key are
kept, so Start streaming sends again." under it, and offers **Start streaming**, which
sends to it again with the same key. **Remove** is different: it stops and
forgets the address and the key.

A stopped destination stays stopped after the mixer restarts. It is written to
`godwinmix.runtime.toml` with `enabled = false`, and you can write the same
line in the config for a destination that should start stopped:

```toml
[[outputs]]
id = "youtube"
uri = "rtmp://a.rtmp.youtube.com/live2/abcd-efgh-ijkl-mnop"
enabled = false
```

## From the API

```sh
curl -X POST -H "Authorization: Bearer $GODWINMIX_TOKEN" \
  http://127.0.0.1:8080/api/v1/outputs/youtube/stop
curl -X POST -H "Authorization: Bearer $GODWINMIX_TOKEN" \
  http://127.0.0.1:8080/api/v1/outputs/youtube/start
```

Both answer with the output record. After a stop its `state` is `stopped`.
Stopping one that is already stopped, or starting one that is already
sending, changes nothing and is not an error. Neither is marked destructive,
because `output.start` undoes `output.stop` with the same key, the way a
channel destination's switch does. A core started with `--rehearsal` refuses
`output.start`, the same way it refuses `output.add`.

From a terminal, `gmx ctl output stop youtube` and `gmx ctl output start
youtube`. An agent has `stop_output` and `start_output`.

A live output's record carries `live_secs`, how long it has been live without
a break, which is where the page's clocks come from.

## Open a page on a mixer that is already streaming

When the page opens, or the desktop app's window comes back from the
background, and the mixer is already streaming, recording or receiving, a bar
across the top says so:

> Still streaming from before: YouTube for 1:56:23, recording for 12:04

**Stop all** stops everything outgoing, after the same one question.
**Keep** closes the bar and leaves it all running; the header's red pill still
counts it. **Details** opens What is running.

This is what you see after choosing to keep the desktop app running in the
background, and also when somebody started a stream from another device.

## Close the desktop app

What closing the window does depends on what is running.

**Nothing running.** The app quits completely: the window, the tray icon, the
mixer, its plugins and its helpers all stop. No GodwinMix process is left.

**Anything streaming, recording or receiving.** The app asks first, naming
each thing, with three answers:

* **Stop everything and quit** ends every stream and recording, stops the
  mixer and quits.
* **Keep running in the background** closes the window and leaves the tray
  icon. The mixer carries on.
* **Cancel** leaves the window open and everything as it was.

Quit in the GodwinMix menu, Quit in the tray, Ctrl+Q and the quit buttons on
the page follow the same rule. So does "Quit and stop the mixer".

The project does not need saving before you quit. The mixer writes its scenes,
sources and outputs to its runtime store as they change, so the next start
opens where you left off; File > Save project as is for a copy you want to
keep or move.

## Running in the background

While the window is closed into the background, the tray icon carries a red
dot. Its tooltip starts "GodwinMix is running in the background" and names what
is running, and the top of its menu lists the same things. The menu offers:

* **Show GodwinMix**, which brings the window back; the page then shows the
  bar above;
* **Stop all streaming**, which asks once and stops every stream, watch link
  and recording, keeping each stream key;
* **Quit**, which asks the same question as closing the window.

The tooltip and the dot say what is running from launch on, not only in the
background, so a glance at the notification area always tells you.

## A mixer on another machine

When the app is connected to a mixer on a server, quitting closes this window
and nothing else: the mixer is not this computer's to stop. If it is
streaming, the app says so first and offers **Quit and leave it streaming** or
**Cancel**. To stop the streams, use What is running before you quit.

## See also

* [The desktop app](desktop-app.md)
* [Stream to YouTube, Facebook or Twitch](stream-to-a-platform.md)
* [Record to a file](record-to-a-file.md)
* [The HTTP API](../reference/http-api.md)
