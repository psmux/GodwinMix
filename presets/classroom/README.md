# The classroom preset

A lesson recorded and streamed: one camera, the teacher's screen beside it, a local recording and one destination.

## What it gives you

The teacher on camera with their screen beside them, recorded to this machine and streamed to the school's server at the same time. It runs on the laptop already in the room.

## What you need

A webcam, the screen you are already presenting on, and somewhere to record to. The camera and screen plugins are installed for you.

## Three steps

1. `gmx preset apply classroom`.
2. Check `godwinmix.toml`: the camera line says `/dev/video0` on Linux; on a Mac or a Windows machine run `gmx ctl device list` and paste the name it prints. Set the recording folder to somewhere with room on it.
3. `gmx`, then http://localhost:8080. Press `Split` when you are showing something on screen and `Teacher` when you are not.

## When it does not work

**The camera is in use.** Usually something else has it open. Close the video call software and press the camera tile again.

**The screen share is black.** Usually on macOS and Windows the screen plugin asks for a permission the first time. Grant it in the system settings and restart `gmx`.

**The recording file is tiny.** Usually the disk filled up, or the lesson stopped before the file was closed. `gmx ctl output stop recording` closes it cleanly; pulling the power does not.


## What is in this directory

| File | What it is |
|---|---|
| `gmx-plugin.toml` | the manifest: the plugins this preset needs, and where its config, layout and scenes are |
| `config/godwinmix.toml` | the mixer's configuration, with every line you have to change near the top |
| `config/layout.json` | which panels go in which slot of the web UI |
| `scenes/` | the scene documents this preset uses, one file each |
| `README.md` | this page |

Copy this whole directory to make your own. `presets/README.md` says how.
