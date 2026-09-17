# The default preset

Two RTMP cameras in, one RTMP destination out, a media library and the multiview. What GodwinMix does out of the box.

## What it gives you

Two cameras arriving over RTMP, one destination going out, and the multiview so you can see both at once. Nothing else is running, which is why it starts on a Raspberry Pi.

## What you need

A machine on the same network as your cameras or encoders, and one RTMP address to send to. No plugins to install.

## Three steps

1. Pick a setup on the welcome page, or apply this one with `gmx preset apply default`.
2. Add your camera: **Add a source**, then its address, or drop a video file onto the page.
3. Press its tile to put it on air. **Destinations**, then **Edit**, sends the programme somewhere other than this machine.

## When it does not work

**A camera tile is black.** Usually the camera is not publishing yet. The Sources panel says what each one is doing; a source that says `connecting` has nothing arriving at that address.

**The destination says reconnecting.** Usually the address or the stream key is wrong, or the far end is refusing you. The alerts panel carries the reason the server gave.

**Nothing is on air.** Usually press a camera tile. The slate is what goes out until you do.
