# Internet radio: send the sound out, or play a station in

## Send the programme's sound to listeners

People who only want to listen (in the car, on a phone with a weak signal, on
a smart speaker) are served best by an Icecast radio stream: a tenth of the
bandwidth of video, and every radio app plays it.

1. Get a mount from a streaming host, or run Icecast yourself. You need the
   server's address, its port (usually 8000), a mount name and the source
   password.
2. Install the plugin once: in the page, open **Plugins** and add `icecast`.
3. Press the palette button (the `⌘K` at the top right), choose
   **Add an output**, then **Icecast radio**.
4. Paste the whole address the host gave you, such as
   `icecast://source:password@radio.example.com:8000/live.mp3`, or fill in the
   server, port, mount and password separately.
5. Leave **Format** at MP3 unless you know your listeners' players take Opus,
   and **Bitrate** at 128 for speech and music alike.
6. Press **Start sending**.

Listeners open `http://radio.example.com:8000/live.mp3`. Only the sound is
sent; the picture never leaves the mixer on this output. Encoding it takes a
few percent of one core.

## Play a station, or any audio stream, as a source

Add a source of type **Icecast** in **Streams and feeds**, with the station's
stream address (not its web page; the stream is what opens in VLC). It plays
live, and its health shows the song title the station sends.

## When it does not work

* **The output says the server refused it.** The source password or the mount
  is wrong. Streaming hosts often give a separate password for sources.
* **A station plays for a while and stops.** Some stations limit how long one
  listener may stay connected; the source reconnects when the mixer restarts
  it.

Every setting is in the
[network plugins reference](../reference/plugins-network.md#icecastoutput-and-icecastsource).
