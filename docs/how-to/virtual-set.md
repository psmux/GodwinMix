# Put a presenter in a virtual set

A presenter stands in front of a green or blue screen, and the programme shows
them in a designed studio: a picture or a looping clip behind, the presenter
keyed in front of it, and if you have one, a desk or a window frame in front of
the presenter. One step makes the scene; the key is tuned afterwards while it
is on air.

You need three things in the mixer first:

* The camera on the presenter, added as a source.
* The background: a picture (PNG, JPEG, WebP) or a clip, uploaded in the
  **Media** tab. A picture made with an AI image tool works as well as a photo.
  Make it the canvas shape, 16 by 9.
* Optionally, a foreground: a PNG with a transparent background, the canvas
  size, with the desk drawn along the bottom. Upload it the same way.

## From the page

1. In the Scenes panel press **Virtual set**.
2. Pick the **Background**, the **Presenter** camera and, if you have one, the
   **Foreground**. Library files are listed with "(library)" after the name.
3. Press **Create the set**.

The scene appears with the presenter standing on the bottom edge, in the
middle, at nine tenths of the canvas height. The toast says the key colour and
where it came from: found in the camera, or, when no still of the camera was
available, found by the key itself once it is on air.

Take it like any other scene.

## Tune the key

Open the scene in the composer and select the item called **presenter**. Under
**Filters** is its key:

* **Find** looks at a still of the camera and keys on the biggest green or blue
  area in it.
* **Pick** shows a still of the camera. Click the screen in it, somewhere
  evenly lit, and that colour becomes the key.
* **Similarity** decides how far from the key colour still goes fully clear.
  Raise it when the darker parts of the screen show through as a tint; lower
  it when the presenter's own colours start to go.
* **Edge softness** is how wide the band between clear and solid is. Raise it
  for hair, lower it for a hard edge.
* **Spill** takes the screen's colour off the presenter's edges and shoulders.
* **Feather** softens the edge inwards by that many pixels. It never adds a
  halo outside the presenter.
* **Matte left, right, top, bottom** cut away everything past that fraction of
  the picture. Use them for the edges of a screen that does not fill the shot:
  light stands, the top of the frame, the unlit corners. It is the cheapest
  control there is, because nothing outside the matte is looked at.

Every slider applies on air as you drag it, with no rebuild and no gap. Move
and size the presenter on the canvas like any other item.

## Over the API, or from an agent

The same scene is one call, `scene.virtual_set`. Over HTTP:

```sh
curl -X POST http://your-mixer:8080/api/v1/scenes/virtual_set \
  -H 'content-type: application/json' \
  -d '{"background": "newsroom.png", "presenter": "cam1", "foreground": "desk.png"}'
```

Add `-H 'authorization: Bearer <token>'` to this and the calls below when the
mixer has a token. An agent on MCP calls the tool `create_virtual_set` with the
same body.

`background` and `foreground` take a media library file name, a path, or a
source id. A file becomes a source the first time; asking again with the same
file uses that source. Give `"key": "#30b050"` to set the colour yourself,
`presenter_scale` (0.3 to 1) and `presenter_x` (0 to 1) to place the presenter,
and `name` for the scene. The answer carries the scene with every item's box in
pixels, the `key` it wrote, `key_from` (`given`, `guessed` or `auto`) and the
sources it `added`.

The key is the item's filter, named `Key`. Change it with
`scene.item.filter.set` (the tool `set_scene_item_filter`):

```sh
curl -X POST http://your-mixer:8080/api/v1/scenes/item/filter/set \
  -H 'content-type: application/json' \
  -d '{"scene": "Virtual set", "item": "presenter", "filter": "Key", "params": {"spill": 0.8, "matte_left": 0.15}}'
```

`params` is merged into what the key already has. To read a key colour off
the camera, use `source.key_color` (the tool `key_color`): with `x` and `y` it
is the colour at that point, with an empty body it is the screen as a whole.

```sh
curl -X POST http://your-mixer:8080/api/v1/sources/cam1/key_color \
  -H 'content-type: application/json' -d '{"x": 0.1, "y": 0.2}'
```

The scene is the built in `virtual-set` layout, so `scene.create_from` with
`"layout": "virtual-set"` and the sources in the order background, presenter,
foreground makes the same scene without the colour guess. Its key then finds
its own colour from the first frames it sees.

## When it does not look right

* **The presenter is on black.** The programme is composited on a GPU
  graphics entry, which this key does not draw on yet. It falls back to the
  old behaviour, keyed areas black. Use the software graphics entry for a
  virtual set.
* **The whole camera shows, green and all.** The key has not found a colour
  yet: the camera may have been black when it looked. Press **Find** or
  **Pick** once the camera shows the screen.
* **A green tint round the hair.** Raise **Spill**, then **Feather** by one or
  two.
* **The edge of the screen shows.** Pull the matte in from that side.
* **The composer's own picture shows the camera unkeyed.** The composer's
  preview does not run item filters. The programme and its snapshot do; check
  there.

The key's settings, one by one, are in the [chroma key reference](../reference/chroma-key.md).
