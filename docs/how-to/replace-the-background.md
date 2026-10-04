# Replace the background behind a person

Put anything behind the person on camera: a picture, a looping clip, a web
page, another camera. With a green or blue screen behind them, or with none
at all.

| Behind the person | What does the work | Edge | Cost |
|---|---|---|---|
| A green or blue screen | the chroma key, `chroma/filter` | the cleanest: hair, glass, motion blur | under a millisecond a frame, any machine |
| Nothing special: a room, a wall, an office | the cutout, `matte/filter`, a matting model | good; fine strands of hair can soften | see below |

Both draw the same way: the person is a transparent item in the scene, and
whatever is below them in the stack shows through. A desk or a lower third
above them stays in front.

## The quickest way: New scene

1. In the Scenes panel, press the **▾** beside **New scene** and choose
   **Presenter in front of a new background**.
2. Pick the **Background** (upload a picture or a clip in the **Media** tab
   first, or pick a source), the **Presenter** camera and, if you like, a
   **Foreground** such as a desk: a PNG with a transparent background.
3. Under **Behind the presenter** choose **A green screen**, **A blue
   screen** or **No screen: cut the person out**.
4. Press **Make the scene**, and take it like any other scene.

The person stands on the bottom edge, in the middle, at nine tenths of the
canvas height. Move and size them like any other item.

## On a scene you already have

Open the scene in the composer, select the camera's item, and under
**Filters** add **Chroma key: a green or blue screen** or **Background
cutout: no screen needed**. Then put a picture, a clip or a page in the scene
below the camera's item. Every setting applies on air as you drag it.

## How the cutout runs, on any machine

The cutout runs a matting model through ONNX Runtime, on the best device the
machine has, and falls back to the CPU on any machine:

| Platform | GPU path | Always |
|---|---|---|
| Windows | DirectML: any Direct3D 12 GPU, Intel, AMD or NVIDIA | the CPU |
| macOS | CoreML: the GPU and the Neural Engine | the CPU |
| Linux | CUDA or OpenVINO, with a runtime built for them | the CPU, x86 and ARM, a Raspberry Pi included |

Two models ship, both under the Apache License 2.0:

| Model | Setting | Measured on an Intel Arc 140T laptop |
|---|---|---|
| MediaPipe selfie segmentation | `quality = "fast"` | 5.6 ms a frame on the CPU, 4.7 ms on the GPU |
| MODNet portrait matting | `quality = "fine"` | 29 ms a frame on the GPU, 120 ms on the CPU |

`quality = "auto"`, the default, takes the fine model when there is a GPU and
the fast one when there is not.

The model never holds the picture up. It runs on a thread of its own and
answers for as many frames as it can; the camera keeps its full frame rate and
the edge follows a frame or two behind. A GPU takes a few seconds to prepare
the first time; until the first answer the camera is drawn whole.

## Tune the cutout

Under the cutout in the composer, or with `scene.item.filter.set` on the item:

* **Model**: automatic, fast or fine. See above.
* **Runs on**: automatic, the GPU, or the CPU. Use the CPU when the GPU is busy
  with something else, an encoder say.
* **Edge position**: lower keeps more around the person, higher keeps less.
* **Edge softness**: the width of the soft band at the edge. 0 is a hard edge.
* **Steadiness**: how much of the last frame's edge carries into the next. The
  edge shimmers less, and follows fast movement a little later.
* **Feather**: softens the edge inwards by that many pixels.
* **Matte left, right, top, bottom**: cut away the edges of the picture,
  whatever the model says. A lamp or a doorway the model takes for part of
  the person goes this way.

Every setting is in the [cutout reference](../reference/cutout.md).

## Over the API, or from an agent

The whole scene is one call, `scene.create_from` (the tool `create_scene_from`):

```sh
curl -X POST http://your-mixer:8080/api/v1/scenes/create_from \
  -H 'content-type: application/json' \
  -d '{"sources": ["beach.mp4", "cam1"], "layout": "virtual-set", "name": "Beach", "settings": {"screen": "none"}}'
```

`screen` is `green` (the default), `blue` or `none`. On an item you already
have, add the filter with `scene.item.filter.add`:

```sh
curl -X POST http://your-mixer:8080/api/v1/scenes/item/filter/add \
  -H 'content-type: application/json' \
  -d '{"scene": "Interview", "item": "cam1", "type": "matte/filter", "params": {"quality": "fine"}}'
```

A machine that cannot run the cutout is told why and what to install, and
nothing is changed.

## On a server

The container image carries the models and the runtime. A server installed
from the release archive gets them with one script from a checkout:

```sh
dev/fetch-models.sh --models /usr/local/share/godwinmix/models \
                    --runtime /usr/local/lib/godwinmix
```

Or install ONNX Runtime 1.20 or newer yourself and point `ORT_DYLIB_PATH` at
its library, and `GODWINMIX_MODELS_DIR` at a folder holding `selfie.onnx` and
`modnet.onnx`.

## Use a model of your own

Any model that takes one picture and answers one matte can be used. Put
`name.onnx` in `~/.godwinmix/models` (on Windows,
`%USERPROFILE%\.godwinmix\models`) with `name.json` beside it, saying the size
it takes and how its input is scaled:

```json
{"width": 512, "height": 288, "mean": 0.5, "std": 0.5}
```

Then set `model = "name"` on the cutout. The picture goes in as
1 x 3 x height x width, RGB, `(value / 255 - mean) / std`, and the first output
is read as the matte, 0 to 1.

Robust Video Matting gives a finer, steadier edge than either model that
ships, but its weights are under the GPL 3.0, so GodwinMix does not carry
them. Its recurrent form takes more than one input and is not read yet.

## When it does not look right

* **The person is on black.** The programme is composited on a GPU graphics
  entry, which does not draw transparent items yet. Use the software graphics
  entry for a scene with a cutout or a key.
* **The camera shows whole, room and all.** The model has not answered yet,
  or could not start. The mixer's log says which, on a line starting "the
  cutout".
* **Bits of the room stay.** Raise **Edge position**, or pull a matte in from
  that side.
* **The edge flickers.** Raise **Steadiness**, or use the fine model.
* **Fast movement leaves a trail.** Lower **Steadiness**.
