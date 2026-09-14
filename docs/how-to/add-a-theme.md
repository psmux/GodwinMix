# Add a theme

A theme is one CSS file of custom properties. There is no build step, no
preprocessor and no JavaScript in it. Copy a file, change some colours, done.

## Four steps

1. Copy `ui/themes/dark.css` to `ui/themes/midnight.css`.
2. Change the values. Leave the property names alone.
3. Add one line to the list at the top of `ui/shell/theme.js`:

   ```js
   { id: "midnight", title: "Midnight", href: "themes/midnight.css" },
   ```

4. Add the file to the `ASSETS` table in `crates/godwinmix/src/ui.rs` so it
   ships in the binary.

Reload. It is in the theme picker under Settings, and the choice is remembered
per device.

While you are working, skip step 4 and point the core at your checkout instead:

```toml
[control]
ui_dir = "/home/you/GodwinMix/ui"
```

Files are then read from disk at request time, so a save and a reload is the
whole loop.

## The properties that matter

`ui/themes/base.css` defines the complete set on `:root` and then uses nothing
else, so a theme that redefines three properties is valid and one that redefines
all of them is complete. These are the ones you will actually reach for.

| Property | What it paints |
|---|---|
| `--bg` | the page behind everything |
| `--panel`, `--panel2`, `--raise` | surfaces, back to front |
| `--line`, `--line-soft` | borders, loud and quiet |
| `--text`, `--dim`, `--faint` | text in three weights of attention |
| `--live` | what is going out, and nothing else |
| `--bad` | faults, deliberately duller than `--live` |
| `--ok`, `--warn` | green and amber |
| `--accent`, `--select`, `--select-fill` | focus, primary buttons, selection, the marquee |
| `--kind-camera`, `--kind-stream`, `--kind-file`, `--kind-page`, `--kind-graphic`, `--kind-other` | the default tile colour per kind |
| `--radius`, `--gap`, `--pad`, `--tile-w`, `--border` | geometry |
| `--anim`, `--anim-take` | motion. Both `0s` gives a still interface |

The full table, with the geometry and type properties, is in
`ui/themes/README.md`.

## The one rule

Red means going out. `--live` is spent on the tally, the on air tile's frame,
the ad pill and the top of a meter. If errors are also red, an operator glancing
across the room cannot tell a camera that failed from a camera that is live.
Faults use `--bad`, and `--bad` has to be visibly duller.

## Four themes ship

* `dark.css`, the default. Near black, so a lit studio picture is the brightest
  thing on the screen.
* `light.css`, for a bright room and for a projector.
* `high-contrast.css`, pure black behind white, thicker borders, every text pair
  above 7:1. It also changes `--border` and the kind colours, which is the
  example to copy if your theme needs more than a palette swap.
* `system.css`, which follows the operating system by redefining only the light
  palette inside `@media (prefers-color-scheme: light)`.

## A theme inside a preset

A theme is a product the same way a preset is, and the quickest way to ship one
is inside a preset. Put `theme.css` beside the manifest and name it:

```toml
[provides.preset]
theme = "calm"                     # the id, and what Settings shows
theme_css = "theme.css"            # the file, relative to the preset root
```

That is the whole of it. `gmx preset apply` writes the name into the `[ui]`
section, `core.info` carries it, and the page loads the stylesheet from
`/presets/<name>/theme.css`, which the core serves out of the preset it was
applied from. No rebuild, no entry in `ui/themes/`, and no line in
`shell/theme.js`.

Four of the six official presets do this: `calm` in `church` (warm, low
contrast between panels, for a screen seen from a few metres away), `daylight`
in `classroom` (a light room with a projector on), `neon` in `esports` and
`broadcast` in `broadcast`. Each is about twenty five lines of custom
properties. Copy one:

```sh
cp presets/church/theme.css presets/my-church/theme.css
```

A preset whose `theme` is not one of the four built in ones and which ships no
`theme_css` is refused by the loader, with the names it could have used. That is
deliberate: a preset that applies cleanly and then renders unstyled is worse
than one that will not apply.

The operator still wins. A preset chooses where somebody starts; the moment they
pick another theme in Settings, their choice is what loads. Applying a different
preset is a deliberate act and does move it.

See `ui/themes/CONTRIBUTING.md` for the paragraph version of that, and
[the presets reference](../reference/presets.md) for the rest of the manifest.
