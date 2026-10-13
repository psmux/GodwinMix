# Key out a green or blue screen on any item

Put a chroma key on a camera that is already in a scene, so whatever is under
it in the scene shows through where the screen was. For a whole presenter set
made in one step, with a background and a desk, see
[put a presenter on a green screen into a designed studio](green-screen-presenter.md).

You need a scene with the keyed camera in it and something under the camera:
a picture, a clip, another camera. The key works on the default software
compositor, on any machine that runs the mixer, a Raspberry Pi included.

## From the page

1. Open the scene in the composer and select the camera's item.
2. Under **Filters**, choose **Chroma key: a green or blue screen** in the
   **Add a filter** list.
3. The key starts on `auto`: it looks at the camera for the biggest green or
   blue area and keys on that. If the camera was dark when it looked, press
   **Find** once it shows the screen, or **Pick** and click the screen in the
   still.
4. Take the scene, or press Apply if it is already on air.

Tune it with the sliders under the key. Each one applies on air as you drag
it, with no rebuild and no gap. What each slider does is in
[tune the key](green-screen-presenter.md#tune-the-key).

The composer's own picture does not run item filters, so it shows the camera
unkeyed. Look at the programme, or its snapshot, to see the key.

## Over the API

Add the key with `scene.item.filter.add`. `params` may be empty; every setting
has a default.

```sh
curl -X POST http://your-mixer:8080/api/v1/scenes/item/filter/add \
  -H 'content-type: application/json' \
  -d '{"scene": "Studio", "item": "cam1", "type": "chroma/filter", "name": "Key", "params": {"color": "#30b050"}}'
```

Add `-H 'authorization: Bearer <token>'` when the mixer has a token. The
answer is the scene, with the key in the item's `filters`. Change it on air
with `scene.item.filter.set`, which merges what it is given:

```sh
curl -X POST http://your-mixer:8080/api/v1/scenes/item/filter/set \
  -H 'content-type: application/json' \
  -d '{"scene": "Studio", "item": "cam1", "filter": "Key", "params": {"similarity": 0.45, "spill": 0.8}}'
```

Take it off with `scene.item.filter.remove`, or turn it off and keep its
settings with `"enabled": false` on `scene.item.filter.set`. Either way the
programme keeps running.

## When it is refused

The key is drawn by the software compositor's overlay board. A mixer pinned
to a GPU graphics entry with `[hardware] graphics` has no board, and there the
add is refused with `data.field` set to `hardware.graphics`. Set it back to
`auto` and restart the mixer:

```sh
gmx ctl rpc config.set '{"values": {"hardware.graphics": "auto"}}'
```

## What it costs

A key costs CPU only while it is on: the programme is composited the same
way with or without one, and nothing changes when the last key comes off. One
keyed camera on a 1920x1080 programme at 30 frames a second measured 14.7
percent of a core on an Apple M4 Pro and about 45 on a busy Windows laptop.
On a small machine, a 1280x720 canvas costs about half that. The figures, and
how to measure them on your own machine, are in the
[chroma key reference](../reference/chroma-key.md#cost).
