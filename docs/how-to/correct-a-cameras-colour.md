# Correct a camera's colour

Brighten a dark room, take the blue out of a cheap webcam, or make two cameras
match, with a colour correction on the item that shows the camera. It runs in
the mixer, on the CPU, on any machine a Raspberry Pi included, and every slider
changes the picture on air as you drag it.

## From the page

1. Open the scene in the composer and select the camera's item.
2. Under **Filters**, choose **Colour: brightness, contrast, saturation and
   hue** in the **Add a filter** list.
3. Move the sliders. **Reset** puts every one back to 0, which changes nothing.

The composer's own picture does not run item filters. Look at the programme,
or its snapshot, to see the result.

## Over the API

```sh
gmx ctl rpc scene.item.filter.add '{"scene":"Studio","item":"cam1","type":"color/filter","params":{"brightness":0.1,"saturation":0.2}}'
```

On a whole source rather than one item, or on the programme, use
`filter.add` with `source` or `programme`, as for any video filter.

## Settings

| Setting | Range | What it does |
|---|---|---|
| `brightness` | -1 to 1 | lifts or lowers the whole picture |
| `contrast` | -4 to 4 | spreads the picture away from the middle grey, or pulls it in |
| `gamma` | -3 to 3 | lightens (above 0) or darkens (below 0) the middle tones, leaving black and white where they are |
| `saturation` | -1 to 5 | -1 is black and white, 0 is as it was, above 0 is more colourful |
| `hue_shift` | -180 to 180 | turns every colour round the colour wheel, in degrees |

Every setting is 0 by default. The names and ranges are the ones OBS's colour
correction uses, so one brought across by an [OBS import](import-from-obs.md)
keeps its look. OBS's `opacity`, `color_add` and `color_multiply` come across in
the document and are not applied yet.

## What it costs

Measured on an Apple silicon MacBook at 1920x1080 and 30 frames a second:
nothing with every setting at 0, 1.3% of one core for brightness and contrast,
and 2.0% with all five. A Raspberry Pi 4 has not been measured yet. The
command that measured it:

```sh
cargo test -p godwinmix-core --release --lib cost_of_a_colour -- --ignored --nocapture
```
