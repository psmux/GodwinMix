# The church preset

A Sunday service on air: two cameras, the lyrics over the picture, the slides, and YouTube and Facebook at the same time.

## What it gives you

Four inputs and two destinations, already configured, plus four scenes you can put on air by pressing a tile. The tiles start as icons rather than pictures, so the machine at the back of the hall keeps its cores for the encoder.

The two cameras start as test patterns, so the stream is real from the first minute and the cameras can wait. `gmx plugin add camera` installs the camera plugin; change `type = "test/source"` to `type = "camera/source"` and nothing else moves.

## What you need

A machine that can reach the internet, and your YouTube and Facebook stream keys. Nothing to install.

## Three steps

1. `gmx preset apply church`. It writes `godwinmix.toml`, the scenes and the layout.
2. Put your two stream keys into the `[[outputs]]` blocks of `godwinmix.toml`.
3. `gmx`. Open http://localhost:8080 on the volunteer's screen, press Wide, and press the red button when the service starts.

## When it does not work

**YouTube says the stream is unhealthy.** Usually the upload is not keeping up. Lower `video_bitrate_kbps` in `[program]` to 3000 and try again; a service looks better steady at 3000 than stuttering at 4500.

**The lyrics do not appear over the picture.** The lyrics page has to have a transparent background. Open its address in a browser: if it is white there, it is white here.

**The slides tile is black.** It is looking for `media/slides.mp4`. Drop a file onto the page, then change the `slides` source's `uri` to the name it lands under.

**The service is over and the stream is still running.** Press the red button again, or `gmx ctl output stop youtube`.
