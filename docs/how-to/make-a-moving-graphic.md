# Make a graphic that moves

A lower third that wipes in, a ticker that crawls, a score bug whose clock
runs, a logo turning in 3D, a looping background behind a presenter. Each is
an HTML template: a web page drawn over the programme with its transparency
kept, told its words by the mixer and told when to come in and go out.

For a graphic that holds still, an [SVG template](write-an-svg-template.md)
is cheaper and is the better choice.

You need the browser renderer. The desktop app has it; a mixer run from a
checkout builds it the first time a web page or an HTML template is added
(see [web page sources](../reference/web-page-sources.md)).

## 1. Start from a starter design

```sh
gmx ctl template list
gmx ctl rpc template.get '{"name": "lower-third-glass"}'
```

The answer's `html` is the whole page. Save it to a file, `ours-strap.html`.
The pack has three lower thirds, two tickers, a live score bug, a logo bug, a
countdown, a 3D logo, a 3D title card, a starting soon slate, two looping
backgrounds and two virtual set backgrounds; the list is in
[graphics for agents](../reference/graphics-for-agents.md#the-starter-pack).

## 2. Change it

Open the file in any editor. The words and colours a show changes are fields,
declared in the `gmx-template` block at the top:

```html
<script type="application/json" id="gmx-template">
{"title": "Ours strap", "category": "lower-third", "out_ms": 600,
 "fields": {"name": {"label": "Name", "default": "Ada Lovelace"}, ...}}
</script>
```

Change the layout in the CSS. Keep three things as they are: the page's
background stays transparent, the graphic is hidden until `.gmx-in` and shown
under it, and nothing is loaded from the network.

## 3. Check it and save it

```sh
gmx ctl rpc template.check "{\"html\": $(jq -Rs . < ours-strap.html)}"
```

`ok: true` means it will draw. Otherwise each problem says what to change.
Then save it into the media library:

```sh
gmx ctl rpc template.save "{\"name\": \"ours-strap\", \"html\": $(jq -Rs . < ours-strap.html)}"
```

`save_template` refuses a page with an error in it and writes nothing. With
`"replace": true` it writes over the old one and every source showing it loads
the new page.

## 4. Put it on air

```sh
gmx ctl source add strap html:ours-strap.html --param fields.name="Grace Hopper"
gmx ctl rpc scene.item.add '{"scene": "studio", "content": {"source": "strap"}, "name": "strap", "visible": false,
  "transform": {"position": {"x": 0, "y": 0}, "frame": {"w": 1920, "h": 1080}},
  "exit": {"type": "hold", "duration_ms": 600}}'
gmx ctl rpc scene.item.set '{"scene": "studio", "item": "strap", "props": {"visible": true}}'
```

Showing the item plays the page's way in. Hiding it plays its way out, and the
`hold` exit keeps the item on the canvas for `out_ms` while it does. Taking a
scene that shows it, or taking it off, does the same.

Change the words on air, with no reload:

```sh
gmx ctl source set strap --param fields.name="Katherine Johnson"
```

To look at it on a scene that is not on air, hold it in with
`--param cue=in`, and put `cue=auto` back when you are done.

## When it goes wrong

* **Nothing shows.** The graphic is out: its item is not on the programme.
  Hold it in with `cue=in` to look. If it still shows nothing, run the check:
  a page with no `.gmx-in` rules, or one whose script fails, shows nothing.
  The page's own errors are in the source's log (`log.set {instance: "strap",
  level: "debug"}`).
* **It covers the picture.** The page paints a background. The check names the
  line.
* **It is slow to come in the first time.** The renderer starts with the
  source, a second or two on a laptop. Add the source before the show, hidden.
* **The way out is cut short.** Give the item `"exit": {"type": "hold",
  "duration_ms": <out_ms>}`.

What each design costs, and the rules a page follows, are in
[graphics for agents](../reference/graphics-for-agents.md).
