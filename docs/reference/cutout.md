# The cutout: `matte/filter`

The person cut out of a camera's picture with no green or blue screen, by a
matting model. Put it on a scene item (`scene.item.filter.add` with
`type: "matte/filter"`) and the item is drawn over whatever is under it, with
the background taken out. Every setting has a default, so `{}` is a working
cutout. For a screen, use [the chroma key](chroma-key.md) instead.

The filter is in builds made with the `matte` cargo feature, which the
desktop app, the release binaries and the container image all are.

## Settings

| Setting | Range | Default | What it does |
|---|---|---|---|
| `quality` | `auto`, `fast`, `fine` | `auto` | Which model: `fast` is MediaPipe selfie segmentation, `fine` is MODNet. `auto` takes `fine` where a GPU will run it and `fast` where none will. |
| `model` | a name | none | A model of your own, `name.onnx` with `name.json` beside it in a models folder. Takes the place of `quality`. Letters, digits, `-` and `_` only. |
| `device` | `auto`, `gpu`, `cpu` | `auto` | Where the model runs. `gpu` is refused on a machine with no GPU accelerator; `auto` falls back to the CPU. |
| `cutoff` | 0 to 1 | 0.5 | Where the edge sits in the model's answer. |
| `softness` | 0 to 1 | 0.3 | The width of the ramp across the edge. 0 is a hard edge. |
| `steady` | 0 to 0.95 | 0.5 | How much of the last mask carries into the next. |
| `feather` | 0 to 20 pixels | 0 | Softens the edge inwards by about this many pixels. |
| `matte_left`, `matte_right`, `matte_top`, `matte_bottom` | 0 to 0.98 each | 0 | The fraction of the picture cut away from each edge, whatever the model says. Each opposite pair must add up to less than 0.98. |

A value outside its range is refused with the field named and its range.
`id` and `opacity` are accepted and ignored.

## Where it looks

ONNX Runtime, in order: `ORT_DYLIB_PATH`; beside the mixer (`onnxruntime.dll`
and `onnxruntime/` on Windows, `Resources/onnxruntime/` in a macOS app,
`lib/GodwinMix/onnxruntime/` and `lib/godwinmix/` on Linux); a checkout's
`target/onnxruntime/`; then the system's own search. Version 1.20 or newer.

Models, in order: `GODWINMIX_MODELS_DIR`; `~/.godwinmix/models`; `models/`
beside the mixer; `share/godwinmix/models`; a macOS app's
`Resources/models`; `lib/GodwinMix/models`; a checkout's `models/`. The first
folder holding the file wins, so a model you add replaces one that shipped.

`dev/fetch-models.sh` puts both in place for a checkout, an installer or a
server. Each download is pinned and checked against its SHA-256.

## A model of your own

`name.json` beside `name.onnx`:

| Key | Default | Meaning |
|---|---|---|
| `width`, `height` | required, 16 to 2048 | The size the model takes. |
| `mean` | 0 | Taken off each channel after scaling to 0 to 1. |
| `std` | 1 | What each channel is divided by after that. Not 0. |

The input is 1 x 3 x height x width, RGB, `(value / 255 - mean) / std`. The
first output is read as 1 x 1 x height x width, 0 to 1, at the input's size.

## How it runs

```text
  frame's thread                      the cutout's thread
  hand over the frame (a reference)   sample it to the model's size
  read the newest mask                run the model on the GPU or the CPU
  lay it over the frame, per block    carry the last mask forward (steady)
  hand the board a keyed picture
```

The frame's thread never waits for the model. On a 1080p canvas its own work
is the mask laid over each 2x2 block, bilinearly, in fixed point: about 2 ms a
frame, measured on an Intel Arc 140T laptop with
`cargo test --release --features matte -- --ignored cost_per_1080p`. The board
draws a cutout exactly as it draws a key, in stacking order with every other
item. Where nothing can draw it (a source's input side, or a programme
composited on a GPU), the cutout is written into the frame over black.

## Refusals

| When | The message says |
|---|---|
| no ONNX Runtime | where it was looked for, and how to install one |
| no model file | the folders it was looked for in, and `dev/fetch-models.sh` |
| a model of your own with no `.json` | what the `.json` needs to say |
| `device = "gpu"` with no GPU accelerator | to use `auto` or `cpu`, or a runtime with DirectML, CoreML, CUDA or OpenVINO |

Adding the filter checks all of these first, and a refusal changes nothing.
