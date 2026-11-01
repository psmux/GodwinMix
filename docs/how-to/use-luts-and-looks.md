# Use LUTs and looks

Give a camera a look, turn a log camera's flat picture into a normal one, or
bring in the `.cube` your colourist made, with a look on the item that shows
the camera. It runs in the mixer, on the CPU, and the picture on air changes
as you pick.

## From the page

1. Open the scene in the composer and select the camera's item.
2. Under **Filters**, choose **Look: LUTs, camera log and graded presets** in
   the **Add a filter** list.
3. The grid shows the camera's own picture through every look. Pick one.
   **None** takes the look off.
4. Move **Amount** to use less of it. 100% is the whole look.
5. Open **Adjust brightness, contrast and colour** to correct the picture
   before the look runs. These are the same sliders as the colour correction.

The tabs sort the looks:

* **Looks** are grades that ship with the mixer: warm, cool, teal and orange,
  faded film, bleach bypass, high contrast, vivid, black and white, sepia,
  and day for night.
* **Camera log** turns a log recording into Rec.709: Sony S-Log3, Panasonic
  V-Log, Canon C-Log3, ARRI LogC3, Fujifilm F-Log, Nikon N-Log, and HLG from
  a phone or an HDR camera. Pick the one your camera is set to.
* **Imported** holds the LUTs you brought in.
* **Presets** holds the looks you saved.

When the item has no camera picture yet, the grid shows a test card of sky,
foliage, skin tones and greys instead.

## Import a LUT

Press **Import LUT** and choose the file, or drop it on the look's controls.
It reads:

* `.cube`, which is what DaVinci Resolve, Adobe and most camera makers write:
  1D, 3D, or a 1D shaper before a 3D cube, up to 65 points a side.
* `.3dl`, from Lustre, Nuke and Flame.
* `.png`: OBS's 512 by 512 square, or a Hald CLUT.

The mixer keeps a copy as a `.cube` in the `luts` folder of the graphics
library, so the original file can go. The imported look is picked straight
away and shows under **Imported**.

## Save a preset

Pick a look, set the amount and the colour sliders, press **Save as
preset**, type a name and press **Save**. The preset holds all three, and
picking it on another camera gives that camera the same look. Saving under a
name that already exists replaces it.

## Over the API

```sh
gmx ctl rpc lut.list
gmx ctl rpc scene.item.filter.add '{"scene":"Studio","item":"cam1","type":"lut/filter","params":{"lut":"teal-orange","amount":0.7}}'
gmx ctl rpc lut.import '{"path":"/Users/me/Downloads/Kodak 2383.cube","name":"kodak"}'
gmx ctl rpc lut.preset.save '{"name":"church-warm","title":"Church warm","lut":"warm","amount":0.6,"saturation":0.1}'
gmx ctl rpc scene.item.filter.set '{"scene":"Studio","item":"cam1","filter":"lut/filter","params":{"preset":"church-warm"}}'
```

On a whole source, or on the programme, use `filter.add` with `source` or
`programme`, as for any video filter. [LUTs and looks](../reference/luts.md)
has every method and setting.

## From OBS

An OBS **Apply LUT** filter comes across in an [OBS import](import-from-obs.md)
as a look that reads the same file at the same amount. The file has to be on
the mixer's machine at the path OBS had. Import it to keep a copy that does
not depend on that path.

## What it costs

A look of any kind, with or without the colour sliders, is one table the
mixer bakes when it changes, so every look costs the same on a frame.
Measured on an Apple M4 Pro at 1920x1080: 1.9 ms a frame across four cores,
4.5 ms on one. No look costs nothing. A Raspberry Pi has not been measured
yet. The command that measured it:

```sh
cargo test -p godwinmix-core --release --lib lut::apply::tests::cost -- --ignored --nocapture
```

The camera conversions are a sound starting point for a live show, with a
grey card near the middle and highlights that roll off rather than clip.
They are made from each maker's published curve and gamut, and are not the
makers' own display LUTs. Import the maker's LUT if you need theirs exactly.
