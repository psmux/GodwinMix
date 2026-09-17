# The esports preset

A match: four player feeds on a node, the caster's camera, an overlay from a page, replays, and one destination.

## What it gives you

Four player feeds, the caster, and the overlay, with a quad view to cut between and replays to go back to. The player machines send NDI so this box only composites and encodes.

## What you need

A second machine (or four) sending NDI, a gigabit switch between them, and your destination's stream key. Four 1080p60 NDI feeds want a gigabit network to themselves.

## Three steps

1. Pick **Streamer or gaming** on the welcome page, or apply it with `gmx preset apply esports`.
2. Finish the checklist the page puts up: paste your Twitch key, and press Install for the NDI plugin so the player feeds can arrive.
3. Press **Caster** to go on air, then **Quad** when the match starts.

## When it does not work

**Feeds stutter.** Usually the network is the first thing to check, not the mixer. Four 1080p60 NDI feeds are about 500 Mbit; put them on their own switch.

**The encoder is behind.** Usually `gmx ctl status` shows the frame interval. 1080p60 with no GPU is more than a laptop can do; drop `fps` to 30 in `[canvas]`, or run this on a box with a GPU.

**The overlay is a frame late.** Usually it is a browser page and it will be. Put anything that has to be frame accurate in the scene as a graphic, not in the page.
