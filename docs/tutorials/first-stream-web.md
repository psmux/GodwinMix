# Your first stream, from the web page

Ten minutes, one camera or one video file, one destination.

## 1. Open the page

Start the mixer, then open `http://<the machine>:8080/` in any browser. On the
same machine that is `http://localhost:8080/`.

If it asks for a token, paste the one the mixer was started with. It is the
value of `GODWINMIX_TOKEN`, or the `token` line under `[control]` in the config
file. It is saved on this device, so you are asked once.

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

The page opens in Studio mode: Preview on the left with a green frame,
Programme on the right with a red one, and **Take** between them. Preview
already shows a scene, the one it expects you to take next.

Click the tile. It goes into Preview. Nothing has gone out yet.

Press **Take**, or Space. Preview goes to Programme with the transition named
under Take, the bar at the top turns red and names it, and that is now the
programme. **Cut** does the same at once, with no transition. Press `0` to cut
to black. Number keys `1` to `9` put the first nine scenes in Preview.

That is the whole idea: one picture is always on, you line up the next one in
Preview, and Take changes which one is on. If you would rather a click went
straight on air, press **Studio mode** under the monitors to turn it off.

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
* **Studio mode**, which the page opens in: clicking a scene or a source puts
  it in Preview, and the big Take button between the two monitors (or Space)
  sends it live. With nothing clicked, Preview shows the scene it expects you
  to take next. The Studio mode button under the monitors turns it off, and
  then a click puts a scene straight on air.

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
