# Build your own graphics gallery with an AI agent

Your agent designs a lower third, a background, a ticker, a corner bug, a
title card or a whole virtual set. It saves each one into the mixer's
Graphics gallery, looks at a picture of it and fixes what is wrong. You open
**View > Graphics**, see every one as a card, and put it on air with two
clicks. This works with Claude Code, opencode, pi or any other agent, with a
large model or a small free one, on Windows, macOS and Linux.

What you need: a running GodwinMix and an agent connected to it. If you have
not connected one yet, **Help > Connect an AI agent** gives you the lines to
paste, and [Connect an AI agent](connect-an-ai-agent.md) explains them.

## 1. Ask for a graphic

The quickest start is in the gallery itself. Open **View > Graphics**, press
**Make one with an AI agent**, pick a kind (lower third, ticker, background,
virtual set, bug, title card), type a few words about your show and press
**Copy the prompt**. Paste it into your agent.

The prompt tells the agent which tools to call and in what order, which is
what makes a small model reliable at this. You can also write your own:

```
Design a lower third for a red and white evening news called NEWS 24.
Save it with save_graphic, look at it with preview_graphic, fix it until
it reads well, and tell me its id.
```

## 2. What the agent does

Five tools carry the whole loop. They are behind `search_tools` in the MCP
tool list and can be called by name; an agent without MCP runs the same ones
from a shell as `godwinmix tool <name> '<json>'`.

```
save_graphic    {"name": "Storm warning strap", "file": "storm.svg", "tags": "news, weather"}
preview_graphic {"id": "storm-warning-strap"}
place_graphic   {"id": "storm-warning-strap", "values": {"headline": "Coast road closed"}}
show_graphic    {"id": "storm-warning-strap"}
list_graphics   {"query": "red lower third"}
```

`save_graphic` takes one graphic of any kind in one call. Give it a `name` and
one of these:

| Give | It becomes |
|---|---|
| `svg`, a whole SVG with `{{fields}}` in it | a template, its words changeable on air |
| `svg` with no fields | a still picture, sharp at any size |
| `html`, a whole page with its CSS inline | a web graphic, transparent where the page has no background |
| `data`, a PNG, WebP, WebM or MOV as base64 | a picture or a clip, with its transparency |
| `file`, a path | whatever the file is: an SVG, a page, a picture, a clip, a folder or a zip |
| `source`, `{"uri": "ticker:", "params": {"items": [...]}}` | a ticker |
| `set`, `{"background": ..., "foreground": ..., "settings": {...}}` | a virtual set |

A path is the agent's own: when the mixer is on the same machine it reads the
file there, and when it is not, `godwinmix mcp` and `godwinmix tool` send the
bytes for it. Add `tags`, a `description` and a `zone` (`lower-third`, `full`
for a background, `bug`, `bottom`, `center`) and the gallery can file it; leave
the zone out and it is worked out from the picture.

`preview_graphic` answers with a picture of the graphic where it would land on
a 16:9 screen, over a grey checkerboard where it is transparent. A model reads
it and fixes what it sees: words outside their panel, something cut off at an
edge, text too small to read. It saves again with `"replace": true`, and every
source already showing the graphic is drawn again on air. Under `godwinmix
tool` the picture is written to a file and the path printed, so an agent with
a shell can open it.

## 3. Put it on air

Every card in **View > Graphics** has one main button, and it moves on as you
press it:

* **Add** puts the graphic on the scene on air, hidden, in its place: a lower
  third low on the left inside title safe, a background under everything, a
  bug in the top right corner, a ticker along the bottom.
* **Take live** shows it. It comes in the way its zone suits (a lower third
  slides in from the left) and, if its scene was not the one on air, that
  scene is taken.
* **Take off** hides it again.

A set's button is **Make scene**: it makes a new scene with the set's
background behind the camera on air, its foreground in front, and the
presenter keyed. Then **Take live** takes that scene.

**Edit** on a graphic with fields opens its words and colours beside a live
picture of them; **Save** keeps them with the graphic and changes them on air
where it is showing. A shipped graphic is copied first, since the pack never
changes. The **⋯** menu duplicates, exports and deletes.

On a phone the same gallery is two cards across with buttons a thumb can hit,
and a long press on a moving graphic plays it.

## 4. Bring in graphics made elsewhere

**Import**, or a drop of files onto the gallery, takes an SVG, an HTML page or
a zip of a page with its files, an OGraf package, a PNG, WebP or JPEG, a WebM
or MOV, and a gallery export. Each file is checked on its own. Any that are
refused are listed with what was wrong and what to do about it, and the rest
are added. An agent does the same with `import_graphics {"path": "..."}`,
which takes a folder of files as easily as one.

## 5. Share a look

**Export** writes your graphics (the ones that did not ship with the mixer)
into one zip and downloads it. Import that zip on another mixer and every
graphic arrives with its name, tags, fields and pictures. An agent calls
`export_graphics {"ids": ["storm-warning-strap", "news-24-studio"]}` for some
of them.

The zip is stored, not compressed. A zip made by hand with the system's
compress command is refused with the fix: export it from a gallery, zip it
again with no compression (`zip -0`), or import the folder itself.

## Where the graphics are kept

In a folder called `graphics` inside the media library, one folder per
graphic with a `graphic.toml` saying what it is. They survive a restart and
can be copied between machines by hand. `[graphics] gallery` in the config
moves the folder. The format is
[the gallery's item format](../reference/gallery-format.md), and every method
is in [the gallery methods](../reference/gallery.md).

## When something does not work

* **"is drawn by a browser, and this mixer's browser is not ready"**: an HTML
  graphic needs the browser source, which is set up the first time a web page
  is added. The message says what it is waiting for or what to install, and
  `data.setup` carries the button that does it.
* **The preview of an HTML graphic is a card with its name**: the mixer draws
  HTML with a browser, not for a preview. Place it on a scene that is not on
  air and look with `preview_frame`, or save a `preview.png` beside its page.
* **"there is already an item"**: save again with `"replace": true` to write
  over it, or give a new name.
* **A transition or an effect has no Add button**: it is played by a take, not
  placed on a scene.
