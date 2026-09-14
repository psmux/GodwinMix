# The esports preset

A match: four player feeds on a node, the caster's camera, an overlay from a page, replays, and one destination.

## What it gives you

Four player feeds, the caster, and the overlay, with a quad view to cut between and replays to go back to. The player machines send NDI so this box only composites and encodes.

## What you need

A second machine (or four) sending NDI, a gigabit switch between them, and your destination's stream key. Four 1080p60 NDI feeds want a gigabit network to themselves.

## Three steps

1. `gmx preset apply esports`.
2. Put your stream key into the `[[outputs]]` block, and run `gmx ctl device list` to get the NDI names of the four player machines into the `[[sources]]` blocks.
3. `gmx`, then http://localhost:8080. `Quad` shows all four, `Full` puts one player up, `Pip` puts the caster in the corner.

## When it does not work

**Feeds stutter.** Usually the network is the first thing to check, not the mixer. Four 1080p60 NDI feeds are about 500 Mbit; put them on their own switch.

**The encoder is behind.** Usually `gmx ctl status` shows the frame interval. 1080p60 with no GPU is more than a laptop can do; drop `fps` to 30 in `[canvas]`, or run this on a box with a GPU.

**The overlay is a frame late.** Usually it is a browser page and it will be. Put anything that has to be frame accurate in the scene as a graphic, not in the page.


## What is in this directory

| File | What it is |
|---|---|
| `gmx-plugin.toml` | the manifest: the plugins this preset needs, and where its config, layout and scenes are |
| `config/godwinmix.toml` | the mixer's configuration, with every line you have to change near the top |
| `config/layout.json` | which panels go in which slot of the web UI |
| `scenes/` | the scene documents this preset uses, one file each |
| `README.md` | this page |

Copy this whole directory to make your own. `presets/README.md` says how.
