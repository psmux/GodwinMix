# The church preset

A Sunday service on air: two cameras, the lyrics over the picture, the slides, and YouTube and Facebook at the same time.

## What it gives you

Four inputs and two destinations, already configured, plus four scenes you can put on air by pressing a tile. The tiles start as icons rather than pictures, so the machine at the back of the hall keeps its cores for the encoder.

The two cameras start as test patterns, so the stream is real from the first minute and the cameras can wait. The page offers to install the camera plugin, and the picker swaps a test pattern for a real camera without anything else moving.

## What you need

A machine that can reach the internet, and your YouTube and Facebook stream keys. Nothing to install.

## Three steps

1. Pick **Church service** on the welcome page, or apply it with `gmx preset apply church`.
2. Finish the checklist the page puts up: paste your YouTube key, paste your Facebook key, and press Install for the camera plugin if you want real cameras.
3. Press **Wide** to put a picture on air, then the red button when the service starts.

## When it does not work

**YouTube says the stream is unhealthy.** Usually the upload is not keeping up. Lower `video_bitrate_kbps` in `[program]` to 3000 and try again; a service looks better steady at 3000 than stuttering at 4500.

**The lyrics do not appear over the picture.** The lyrics page has to have a transparent background. Open its address in a browser: if it is white there, it is white here.

**The slides tile is black.** It is looking for `media/slides.mp4`. Drop a file onto the page and pick it on the `slides` source.

**The service is over and the stream is still running.** Press the red button again, then stop the destination in the Destinations panel.
