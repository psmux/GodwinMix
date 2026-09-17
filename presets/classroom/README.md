# The classroom preset

A lesson recorded and streamed: a camera on the teacher, the screen beside it, the slides from a file, going to a recorder and to one address.

## What it gives you

Three inputs, two destinations and three scenes, on a 720p25 canvas that runs on the laptop already in the room. Tiles start as stills, refreshed when you click one, which is what a laptop on battery wants.

The camera and the screen start as test patterns, so the lesson can be recorded before the room's hardware is sorted out. The page offers to install the camera and screen plugins, and the picker then lists the devices this room actually has.

## What you need

A machine in the room, and somewhere to send to. Most schools run mediamtx on the same machine, which is what the two `[[outputs]]` addresses point at.

## Three steps

1. Pick **Classroom** on the welcome page, or apply it with `gmx preset apply classroom`.
2. Finish the checklist the page puts up: press Install for the camera and screen plugins, so the test patterns become the real room.
3. Press **Teacher** to go on air, then **Split** to put the shared screen beside it.

## When it does not work

**Both destinations say reconnecting.** Nothing is listening at `rtmp://127.0.0.1:1935`. Start mediamtx, or change the addresses to a server that is running.

**The recording has no sound.** Check the meter on the tile and the fader behind its gear.

**The slides tile is black.** It is looking for `media/lesson.mp4`. Drop the file onto the page and change the `slides` source's `uri` to the name it lands under.

**The picture is too soft on a projector.** Raise `video_bitrate_kbps` in `[program]`, or raise the canvas to 1920x1080 before the first lesson. The canvas cannot change once the mixer is running.
