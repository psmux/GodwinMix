# Your first stream with a preset

This is the church path, timed: from a machine with nothing on it to a picture
going out to YouTube, in about ten minutes, most of which is YouTube's.

You need a machine with GodwinMix on it and a YouTube account. No cameras yet.
The preset starts with test patterns on purpose, so you find out whether the
stream works before you find out whether the cameras do.

## 1. Apply the preset (1 minute)

```sh
gmx preset apply church --dry-run
```

It prints what it would do and writes nothing. Read it. The part that matters:

```
plugins
  MISSING  camera@^1        `gmx plugin add camera`
  have     browser@^1       browser/source
  have     rtmp@^1          rtmp/source, rtmp/output
```

The camera plugin is not here yet and that is fine: the two cameras in this
preset are test patterns until it is. Everything else is built in.

```sh
gmx preset apply church
```

Three files now sit beside you: `godwinmix.toml`, `godwinmix.scenes.json` and
`godwinmix.runtime.toml`. The first has the preset's own comments in it, which
are the instructions for the rest of this page.

## 2. Get your stream key (3 minutes, mostly YouTube's)

Open YouTube Studio, then Create, then Go Live. Pick "Streaming software" if it
asks. The page shows a **Stream URL** and a **Stream key**. Leave it open.

Open `godwinmix.toml` and find the two `[[outputs]]` blocks near the bottom.
Put the key on the end of the YouTube URL, where `YOUR-STREAM-KEY` is:

```toml
[[outputs]]
id = "youtube"
type = "rtmp/output"
uri = "rtmp://a.rtmp.youtube.com/live2/abcd-efgh-ijkl-mnop-qrst"
```

Delete the whole `facebook` block if you are not using it. An output pointing at
a key that does not exist reconnects forever, which is noise in the log rather
than a fault, but there is no reason to have it.

## 3. Start it (30 seconds)

```sh
gmx
```

It prints the address it is listening on. Open `http://localhost:8080`.

The page is in the preset's own theme, and the tiles are icons rather than
moving pictures, because that is what the church preset chose: a machine at the
back of a hall keeps its cores for the encoder. Settings changes it.

## 4. Put something on air (10 seconds)

Press the **Wide** tile. The programme monitor at the top shows the test
pattern, and the tile takes a red frame, which on this page always means "this
is going out".

Go back to YouTube Studio. Within about twenty seconds the preview fills in and
the health indicator turns green. That is your stream.

## 5. Check it is healthy

```sh
gmx ctl status
```

```
program : cam-wide
backend : vtenc_h264_hw (hardware)
source  : cam-wide   live       test://smpte/…
source  : cam-pulpit live       test://ball/…
output  : youtube    connected  0 reconnects
```

`connected` with `0 reconnects` is what you want. A number that climbs means the
upload is not keeping up: lower `video_bitrate_kbps` in `[program]` to 3000 and
restart. A service looks better steady at 3000 than stuttering at 4500.

## What you have

A real stream, with the wrong pictures in it. That is the point: everything
between the camera and YouTube is now known to work, so the next thing you
change is the only thing that can break.

## Next

**Real cameras.** `gmx plugin add camera` installs the camera plugin. Then in
`godwinmix.toml` change the two `test/source` entries to `camera/source` and put
the device into `params`. Nothing else in the file moves.

**The lyrics.** The `lyrics` source points at `http://127.0.0.1:8000/lyrics`.
Point it at whatever your presentation software serves, and give that page a
transparent background: if it is white in a browser it is white on air.

**The slides.** Drag a video file onto the page. It lands in the media library;
change the `slides` source's `uri` to the name it lands under.

**Stopping.** Press the tile again, or `gmx ctl output stop youtube`, then stop
YouTube's end. Ctrl-C stops the mixer.

## If it does not work

**Nothing at `localhost:8080`.** The mixer printed an address; use that one. On
a server, `[control] bind` is `0.0.0.0:8080` in this preset, so use the server's
own address and set a token before it is on a network anybody else is on.

**The output says `reconnecting`.** The key is wrong, or YouTube has not started
its side. The Alerts panel carries the reason the server gave, verbatim.

**A tile is black.** `gmx ctl source list` says what each one is doing. A source
that says `connecting` has nothing arriving at that address.

**Everything else.** `gmx doctor` checks this machine: elements, encoders,
ports, the config and the disk, and exits non zero when something the default
pipeline needs is missing. It is the first thing to run and the first thing to
paste into an issue.
