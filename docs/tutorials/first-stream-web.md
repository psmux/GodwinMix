# Your first stream, from the web page

Ten minutes, one camera or one video file, one destination.

## 1. Open the page

Start the mixer, then open `http://<the machine>:8080/` in any browser. On the
same machine that is `http://localhost:8080/`.

If it asks for a token, paste the one the mixer was started with. Whoever set
the mixer up has it. It is saved on this device, so you are asked once.

On a mixer nobody has set up yet the welcome tiles come up first: a church
service, a classroom, a gaming stream, your OBS scenes, or **Start empty**.
Picking one configures the mixer and then puts up a checklist you finish here,
with a box for each stream key. This page takes the empty path; [Your first
stream with a preset](first-stream-with-a-preset.md) takes the other one.

The bar across the top is the programme: what your audience sees. It says
**black** because nothing is on air yet.

## 2. Add a source

Click **Add** in the Sources panel, or press Ctrl+N.

Pick what you have:

* **Incoming stream** for a camera or an encoder sending RTMP, SRT, RTSP or
  HLS. Type its address, for example `rtmp://192.168.1.20/live/cam1`.
* **Video file** for a clip on the mixer's machine. Type its path.
* **Web page** for lyrics, a scoreboard or a lower third. Type its URL.

Click **Add**. A tile appears in the tray. Its dot goes green when the source is
live; amber means it is still connecting.

You can also drag a file or paste a URL onto the window, which skips the picker.

## 3. Put it on air

Click the tile.

The frame goes red, the bar at the top turns red and names it, and that source
is now the programme. Click another tile to cut to it. Press `0` to cut to
black. Number keys `1` to `9` take the first nine tiles.

That is the whole idea: one picture is always on, and tapping a tile changes
which one.

## 4. Add an output

Nothing is leaving the machine yet. In the Outputs panel click **Add**, pick
**RTMP destination**, and fill in two things:

* **Name**: a short word, like `youtube`. It is what alerts will call it.
* **Address and key**: the full URL your platform gave you, with the stream key
  on the end, for example
  `rtmp://a.rtmp.youtube.com/live2/xxxx-xxxx-xxxx-xxxx`.

Leave **When it drops** on `own` for a server you run, or change it to `cdn` for
YouTube, Facebook or Twitch, which want a gentler reconnect.

Click **Start sending**. The row shows `live` when the connection is up, and the
buffer and reconnect count beside it tell you whether a wobbly link is coping.

You are on air.

## What to try next

* **Rename a tile**: select it and press F2, type, Enter. Right click it for a
  colour.
* **Select several**: Ctrl click to add one, Shift click for a run, or press on
  empty space and sweep. Ctrl+A takes everything, Escape clears it.
* **Undo**: Ctrl+Z. Every delete, rename and drop can be taken back, and the
  dangerous ones offer an Undo button in a toast.
* **Find a command**: Ctrl+K lists every one of them, searchable.
* **Sound**: hover a tile for its fader and mute. The meter is after the fader
  and before the mute, so a muted camera still shows whether it has sound.
* **Turn the pictures down**: the dropdown beside the filter box steps every
  tile between live, snapshot, icon and label. On a Raspberry Pi choose icon:
  the sources keep running and stay takeable, only the tiles stop showing
  moving pictures, and the mixer stops encoding them.
* **Producer mode**: Settings, first tab. Tapping a tile then arms it instead of
  taking it, and Take and Auto buttons appear beside the programme.

## If something is wrong

* **The page says it is disconnected.** The programme keeps going out; only the
  page has lost its connection. It retries on its own.
* **A tile's dot is amber and stays amber.** The source is connecting and not
  arriving. Check the address, and check the Alerts panel: the message names
  what the mixer is waiting for.
* **An output says `reconnecting`.** The destination refused or dropped the
  connection. The buffer figure on that row is how much of a gap the mixer can
  cover before the audience sees one.
* **A button did nothing and a red toast appeared.** Read it. Every error from
  the mixer says what state it is in and what to do next, in a sentence.
