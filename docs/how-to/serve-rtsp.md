# Serve the programme over RTSP

Some equipment only pulls a stream from an address: a hardware decoder, a
network video recorder, a video wall, VLC on a monitoring screen, another
mixer's RTSP input. For those, the mixer serves the programme at
`rtsp://<this machine>:8554/live`.

## Add it

1. Install the RTSP plugin once: in the page, open **Plugins** and add `rtsp`.
   It installs while the mixer runs; nothing goes off air.
2. Press the palette button (the `⌘K` at the top right) and choose
   **Add an output**, then **RTSP server**.
3. Give it a name. Leave **Port** at 8554 unless something else on this
   machine already uses it, and **Path** at `live` unless you want a different
   address.
4. Press **Start sending**.

The port opens now, and closes again when you remove the output. Nothing
listens before.

## Point a player at it

Use this machine's address on the network:

* VLC: **Media, Open Network Stream**, `rtsp://192.168.1.10:8554/live`.
* ffplay: `ffplay -rtsp_transport tcp rtsp://192.168.1.10:8554/live`.
* A decoder or recorder: add an RTSP camera or stream with that address. There
  is no user name or password.

A player that joins starts at the next keyframe, so the picture appears
within one keyframe interval (two seconds unless you changed it).

## When it does not play

* **The player says 404 straight after you added the output.** The programme
  had not reached the output yet. Try again a second later; most players
  retry by themselves.
* **It plays on the same machine but not from another.** A firewall is in the
  way. Ask the player for TCP (VLC: Preferences, Input and Codecs, "RTP over
  RTSP (TCP)"), which uses the one port; UDP needs the player's ports open
  too.
* **The output says another program is using the port.** Choose another port
  in its settings.

Every setting is in the
[network plugins reference](../reference/plugins-network.md#rtspoutput).
