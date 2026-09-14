# The classroom preset

A lesson recorded and streamed: a camera on the teacher, the screen beside it, the slides from a file, going to a recorder and to one address.

## What it gives you

Three inputs, two destinations and three scenes, on a 720p25 canvas that runs on the laptop already in the room. Tiles start as stills, refreshed when you click one, which is what a laptop on battery wants.

The camera and the screen start as test patterns, so the lesson can be recorded before the room's hardware is sorted out. `gmx plugin add camera` and `gmx plugin add screen` replace them: change `type` and put the device into `params`.

## What you need

A machine in the room, and somewhere to send to. Most schools run mediamtx on the same machine, which is what the two `[[outputs]]` addresses point at.

## Three steps

1. `gmx preset apply classroom`. It writes `godwinmix.toml`, the scenes and the layout.
2. Change the two `[[outputs]]` addresses to your school's recorder and stream, or leave them pointing at a local mediamtx.
3. `gmx`. Open http://localhost:8080 and press Teacher to start the lesson.

## When it does not work

**Both destinations say reconnecting.** Nothing is listening at `rtmp://127.0.0.1:1935`. Start mediamtx, or change the addresses to a server that is running.

**The recording has no sound.** Check the meter on the tile and the fader behind its gear.

**The slides tile is black.** It is looking for `media/lesson.mp4`. Drop the file onto the page and change the `slides` source's `uri` to the name it lands under.

**The picture is too soft on a projector.** Raise `video_bitrate_kbps` in `[program]`, or raise the canvas to 1920x1080 before the first lesson. The canvas cannot change once the mixer is running.
