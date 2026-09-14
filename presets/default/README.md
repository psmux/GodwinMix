# The default preset

Two RTMP cameras in, one RTMP destination out, a media library and the multiview. What GodwinMix does out of the box.

## What it gives you

Two cameras arriving over RTMP, one destination going out, and the multiview so you can see both at once. Nothing else is running, which is why it starts on a Raspberry Pi.

## What you need

A machine on the same network as your cameras or encoders, and one RTMP address to send to. No plugins to install.

## Three steps

1. `gmx preset apply default`. It writes `godwinmix.toml` beside you.
2. Open `godwinmix.toml` and put your own destination in the `[[outputs]]` block. If your cameras publish somewhere other than this machine, change the two `[[sources]]` addresses too.
3. `gmx` (or `docker run`). Open http://localhost:8080 and press the camera you want on air.

## When it does not work

**A camera tile is black.** Usually the camera is not publishing yet. `gmx ctl source list` says what each one is doing; a source that says `connecting` has nothing arriving at that address.

**The destination says reconnecting.** Usually the address or the stream key is wrong, or the far end is refusing you. The alerts panel carries the reason the server gave.

**Nothing is on air.** Usually press a camera tile. The slate is what goes out until you do.
