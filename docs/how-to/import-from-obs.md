# Import your scenes from OBS

You have an OBS layout that works. This brings it across in one command, and
tells you plainly what it could not bring.

## 1. Find the collection file

OBS keeps every scene collection as a single JSON file. It is already on your
disk, so there is nothing to export unless you want to.

| System | Where |
|---|---|
| Windows | `%APPDATA%\obs-studio\basic\scenes\` |
| macOS | `~/Library/Application Support/obs-studio/basic/scenes/` |
| Linux | `~/.config/obs-studio/basic/scenes/` |
| Linux, Flatpak | `~/.var/app/com.obsproject.Studio/config/obs-studio/basic/scenes/` |

There is one file per collection, named after it: `Sunday_service.json`. If
you would rather have a copy, OBS's menu bar has **Scene Collection**, then
**Export**, which writes the same file wherever you point it.

Copy the file to the machine GodwinMix is on. Nothing else has to move yet.

## 2. Run the import

Look first, without writing anything:

```
gmx import obs Sunday_service.json
```

That prints the report and stops. When you are happy with it, run it again and
keep the result:

```
gmx import obs Sunday_service.json --out godwinmix.toml --scenes scenes.json
```

`godwinmix.toml` is the source list: every source that came across, with the
plugin that plays it. `scenes.json` is the scene document: every scene, every
item, where each one sits.

Two options are worth knowing about.

**`--canvas 1920x1080`.** OBS keeps the canvas size in its profile, not in the
collection, so the file does not say what it was. The import assumes 1080p and
says so. If yours is 720p or vertical, pass it:

```
gmx import obs Sunday_service.json --canvas 1080x1920
```

**`--source-size "CAM 1=1920x1080"`.** The collection does not record how big
each source is either. That matters in two places: a cropped item, and an item
OBS sized by a scale factor rather than a bounds box. Where the file does not
say, the import measures against the canvas and tells you which items those
were. Repeat the option once per source to get them exact:

```
gmx import obs Sunday_service.json \
  --source-size "CAM 1 (Studio)=1920x1080" \
  --source-size "Webcam=1280x720"
```

## 3. Read the report

```
Imported "Sunday service": 3 scene(s), 11 item(s), canvas 1920x1080.

Sources
  Backdrop            color_source_v3     imported as test/source "backdrop", in 1 item
  CAM 1 (Studio)      v4l2_input          imported as camera/source "cam-1-studio", in 2 items: needs the camera plugin
  Scoreboard          browser_source      imported as browser/source "scoreboard", in 1 item
  Titles              text_gdiplus_v2     skipped, because GodwinMix has no text source...
  Wide                scene               imported as a scene of its own

Filters copied onto each placement
  "Key" (chroma_key_filter_v2) on "CAM 1 (Studio)" -> 2 item(s): Wide / CAM 1 (Studio), Close / CAM 1 (Studio)
  A filter belongs to the placement here, not to the source, so these copies are
  independent from now on: changing one does not change the others.

Worth knowing
  ...
```

Every OBS source gets one line, and the line ends in one of three things.

**imported as ...** It came across whole and it will work. The name in quotes
is the id an operator types and the API uses.

**needs the ... plugin.** The layout came across and the thing that plays it is
not installed yet. Cameras, screen captures and NDI are all in this group. Run
`gmx plugin add <name>` for each one named, and the source starts working with
no other change.

**skipped, because ...** It did not come across, and the reason says why. Two
reasons are common:

* *no equivalent yet.* An OBS plugin GodwinMix does not have. The item is left
  out of the scene. Replace it with something that exists, or ask for the
  plugin.
* *no text source.* Text in GodwinMix is a graphic, not a source. The item is
  still in the scene as a placeholder graphic with your text, font and colour
  in it, waiting for a graphic plugin to point at.

Then two sections that only appear when they apply.

**Filters copied onto each placement.** In OBS a filter belongs to the source,
so a camera keyed in one scene is keyed in every scene. Here a filter belongs to
the placement, which is the thing people spend a nested scene per placement
working around in OBS. The import copies each source filter onto every item
that used it, and this section names each copy. From now on the copies are
independent: changing the key on the wide shot leaves the close shot alone.
That is the behaviour you want, and it is not the behaviour you had, so the
report says it out loud.

**Worth knowing.** Everything that is not about one source: the canvas that was
assumed, the crops that were measured against it, an item whose source is no
longer in the collection.

For a script or an agent, `--report json` prints the same thing as one object.

## 4. Check it, then run it

```
gmx scene validate scenes.json
```

That reports items off the canvas, items completely hidden under another, and
anything that reaches outside the safe areas. None of it stops the scene going
to air; it is there so you can see what moved.

Then install whatever the report asked for, edit the destination in
`godwinmix.toml`, and start:

```
gmx plugin add camera
gmx
```

`gmx plugin add` is not in this release yet. Until it is, a source whose line
says "needs the ... plugin" is in your scene document and in your source list,
and it starts working the day you install the plugin, with nothing else to
change.

## From a page, or anything that speaks the protocol

The mixer can read the collection itself, sent as text, and add the sources in
the same call. This is what a browser does with the file a person picked, so
the file does not have to be on the mixer's machine:

```sh
jq -Rs '{content: ., add_sources: true}' Sunday_service.json \
  | curl -s -X POST localhost:8080/api/v1/scenes/import/obs \
      -H 'content-type: application/json' -d @-
```

The answer is the report above as JSON, plus two lists:

```json
{
  "scenes": ["Main"],
  "sources_added": ["backdrop"],
  "sources_not_added": [
    {"id": "camera", "plugin": "camera",
     "reason": "Camera needs the camera plugin, which is not installed. Install it, then import again or add the source."}
  ]
}
```

A source whose id the mixer already has is not added twice: the imported
scenes use the one that is there, and the reason says so. A source the mixer
refuses, a clip whose file is not on this machine for example, is listed with
the mixer's own reason.

The welcome screen's Import from OBS tile still shows the command line. A drop
zone that sends the file this way arrives in a later change.

## What comes across, exactly

| OBS | GodwinMix |
|---|---|
| a scene | a scene |
| a scene item | an item, with its own transform, crop, opacity and filters |
| a group | an item with children, with the group's transform already multiplied into each child |
| a nested scene | a reference to that scene |
| `pos`, `rot`, `scale` | `transform.position`, `transform.rotation`, `transform.scale` |
| `align` (the bitmask) | `transform.anchor`, as 0, 0.5 or 1 on each axis |
| the seven `bounds_type` values | `transform.frame` plus `transform.fit`: none, stretch, contain, cover, fit-width, fit-height, max |
| `bounds_align` | `transform.align`, one of the nine keywords |
| `crop_left` and friends, in pixels | `crop`, as a fraction of the source, so it survives a canvas change |
| `blend_type` | `blend`, the same seven names |
| `visible`, `locked` | `visible`, `locked` |
| a source filter | a copy on every item that used the source |
| `ffmpeg_source`, `vlc_source` | `file/source`, or `rtmp`, `srt`, `rtsp`, `rtp` or `hls` by the address's scheme |
| `image_source` | `file/source` |
| `browser_source` | `browser/source`, with the url, size, frame rate and css |
| `v4l2_input`, `av_capture_input`, `dshow_input` | `camera/source`, needs the camera plugin |
| `monitor_capture`, `window_capture`, `xshm_input` | `screen/source`, needs the screen plugin |
| `color_source` | `test/source`, as a solid colour |
| `text_gdiplus`, `text_ft2_source` | a placeholder graphic, reported as skipped |
| anything else | skipped, with the OBS type named |

What does not come across: OBS's transitions, hotkeys, profiles, output
settings and audio monitoring. Outputs live in an OBS profile rather than in a
collection, so there is nothing in the file to read; put your destination in
`godwinmix.toml` by hand.

## When it does not work

**"this file is not an OBS scene collection".** You passed OBS's `global.ini`,
a profile, or a `.json` that is something else. The right file is under
`basic/scenes/` and is named after the collection.

**An item is in the wrong place.** Nearly always a source whose size the file
does not record, sized by a scale factor rather than a bounds box. The report
names each one under "Worth knowing". Pass `--source-size "NAME=WIDTHxHEIGHT"`
for those and run it again.

**A crop is wrong.** The same cause and the same fix: a pixel crop needs the
source's size to become a fraction.

**A source is in no scene.** OBS keeps sources that are not placed anywhere.
They are imported into the source list and you can place them, or delete the
lines.

**You want to try again.** The import reads the file and writes new ones. It
changes nothing in OBS and nothing in your running mixer, so run it as many
times as you like.
