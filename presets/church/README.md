# The church preset

Sunday service: two NDI cameras, lyrics from a browser page, slides, YouTube and Facebook at once, and a four button surface for the volunteer.

## What it gives you

A Sunday service on air: the wide camera, the pulpit camera, the lyrics over the picture, and the slides, going to YouTube and Facebook at the same time. The volunteer sees four buttons and nothing else.

## What you need

Two NDI cameras on the same network (or one camera and a phone), your YouTube and Facebook stream keys, and a machine that can reach the internet. The NDI and browser plugins are installed for you by `gmx preset apply`.

## Three steps

1. `gmx preset apply church`. It installs the plugins it needs and writes `godwinmix.toml`.
2. Put your two stream keys into the `[[outputs]]` blocks. Run `gmx ctl device list` to see your cameras' NDI names, and put those into the two `[[sources]]` blocks.
3. `gmx`. Open http://localhost:8080 on the volunteer's screen, press Wide, and press the red button when the service starts.

## When it does not work

**No cameras in the list.** Usually NDI needs both machines on the same subnet with multicast allowed. `gmx ctl device list` shows nothing if the network is blocking it; plugging the camera into the same switch as the mixer is the usual fix.

**The lyrics do not appear over the picture.** Usually the lyrics page has to have a transparent background. Open the address in a browser: if it is white there, it will be white here.

**YouTube says the stream is unhealthy.** Usually the upload is not keeping up. Lower `video_bitrate_kbps` in `[program]` to 3000 and try again; a service looks better steady at 3000 than stuttering at 4500.

**The service is over and the stream is still running.** Usually press the red button again, or `gmx ctl output stop youtube`.


## What is in this directory

| File | What it is |
|---|---|
| `gmx-plugin.toml` | the manifest: the plugins this preset needs, and where its config, layout and scenes are |
| `config/godwinmix.toml` | the mixer's configuration, with every line you have to change near the top |
| `config/layout.json` | which panels go in which slot of the web UI |
| `scenes/` | the scene documents this preset uses, one file each |
| `README.md` | this page |

Copy this whole directory to make your own. `presets/README.md` says how.
