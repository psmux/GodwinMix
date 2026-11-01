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
| `cutoff` | 0 to 1 | 0.6 | Where the edge sits in the model's answer. |
| `softness` | 0 to 1 | 0.2 | The width of the ramp across the edge. 0 is a hard edge. |
| `steady` | 0 to 0.95 | 0.5 | How much small changes carry into the next mask. Large changes follow the new mask immediately. |
| `feather` | 0 to 20 pixels | 0 | Softens the edge inwards by about this many pixels. |
| `matte_left`, `matte_right`, `matte_top`, `matte_bottom` | 0 to 0.98 each | 0 | The fraction of the picture cut away from each edge, whatever the model says. Each opposite pair must add up to less than 0.98. |

A value outside its range is refused with the field named and its range.
`id` and `opacity` are accepted and ignored.

## Where it looks

ONNX Runtime, in order: `ORT_DYLIB_PATH`; beside the mixer (`onnxruntime.dll`
and `onnxruntime/` on Windows, `Resources/onnxruntime/` in a macOS app,
`lib/GodwinMix/onnxruntime/` and `lib/godwinmix/` on Linux); a checkout's
`target/onnxruntime/`; on macOS the installed
`/Applications/GodwinMix.app/Contents/Resources/onnxruntime/`; then the system's own search. Version 1.20 or newer.

Models, in order: `GODWINMIX_MODELS_DIR`; `~/.godwinmix/models`; `~/.cache/gmx-models`; `models/`
beside the mixer; `share/godwinmix/models`; a macOS app's
`Resources/models`; `lib/GodwinMix/models`; a checkout's `models/`; on macOS the installed
`/Applications/GodwinMix.app/Contents/Resources/models`. The first
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

The frame's thread never waits for the model. Small disconnected foreground
components and small enclosed holes are cleaned at model resolution on the
worker. Regions larger than one part in 1500 of the mask survive even when
detached, so a raised hand is not discarded just because it misses the body.
Eight connected neighbours preserve diagonal attachments. Open background
regions are never filled.

The current frame's luma guides the newest mask using a native Rust
[fast guided filter](https://arxiv.org/abs/1505.00996). Linear coefficients
are fitted at one eighth of the frame width and height, with a radius of four
samples and epsilon 0.0004 on luma scaled to 0 to 1. Their bilinear expansion
is evaluated with the average luma of each actual 2x2 block. The edge curve
then produces soft alpha. This happens on the frame's thread; inference and
component cleanup stay on the worker. No new crate is required.

On an Apple M4 Pro at 1920 x 1080, the optimized evaluator measured guided
alpha at 2.04 ms mean and 2.10 ms P95 with the fast model; component cleanup
added 0.37 ms on the worker. With fine, guided alpha averaged 2.49 ms with
a 5.45 ms P95, so this is not a hard 3 ms deadline guarantee. These spans
exclude compositing and I/O. The evaluator compiles the actual refinement
modules with `cargo rustc -p godwinmix-core --features matte --example matte_eval -- -C opt-level=3`;
the linked core library is a development build and the native runtime uses
the CPU. This does not establish full mixer release throughput.

The tighter default cutoff of 0.6 and softness of 0.2 reduce uncertain room
pixels. Feather stays zero because extra inward blur loses hair. Edge colour
reconstruction is not enabled: pulling colours from solid foreground can
spread shirt and skin colours into hair. The guide cannot recover hair the
model misses or distinguish a room patch with the same luma as hair.

The board draws a cutout in stacking order with every other item. Where
nothing can draw it (a source's input side, or a programme composited on a
GPU), the cutout is written into the frame over black.

## Refusals

| When | The message says |
|---|---|
| no ONNX Runtime | where it was looked for, and how to install one |
| no model file | the folders it was looked for in, and `dev/fetch-models.sh` |
| a model of your own with no `.json` | what the `.json` needs to say |
| `device = "gpu"` with no GPU accelerator | to use `auto` or `cpu`, or a runtime with DirectML, CoreML, CUDA or OpenVINO |

Adding the filter checks all of these first, and a refusal changes nothing.
