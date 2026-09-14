# Themes

A theme is one CSS file. No build step, no preprocessor, no JavaScript. You copy
a file, change some colours, and the page looks different.

## Adding one

1. Copy `dark.css` to `midnight.css` in this directory.
2. Change the values. Keep the property names exactly as they are.
3. Add one line to the list in `../shell/theme.js`:

   ```js
   { id: "midnight", title: "Midnight", href: "themes/midnight.css" },
   ```

4. Add the file to the table in `src/ui.rs` so it ships in the binary.
5. Reload. It is in the theme picker in Settings, and the choice is remembered
   per device.

That is the whole process. If step 4 feels like too much, point `ui_dir` in your
config at a directory on disk during development and the files are read at
request time instead:

```toml
[control]
ui_dir = "/home/you/GodwinMix/ui"
```

## What a theme may change

Everything themable is a CSS custom property. `base.css` defines the complete
set on `:root` and then uses only those properties, so a theme that redefines
them all is complete and one that redefines three is still valid: the rest fall
back to the defaults.

| Property | What it paints |
|---|---|
| `--bg` | the page behind everything |
| `--panel`, `--panel2`, `--raise` | surfaces, back to front: slots, cards, button faces |
| `--overlay` | the wash behind a modal and under a tile's hover strip |
| `--line`, `--line-soft` | borders, loud and quiet |
| `--text`, `--dim`, `--faint` | text, in three weights of attention |
| `--on-accent` | text on an accent coloured button |
| `--live` | **only** what is going out: the tally, the on air frame, the ad pill, the top of a meter |
| `--bad` | faults. Keep it duller than `--live` or a failed source reads as a live one |
| `--ok`, `--warn` | green and amber, for states and meter bands |
| `--accent`, `--select`, `--select-fill` | focus rings, primary buttons, the marquee and the selected tile |
| `--kind-camera`, `--kind-stream`, `--kind-file`, `--kind-page`, `--kind-graphic`, `--kind-other` | the default colour of a tile, by what kind of thing it is |
| `--font`, `--num` | the body face and the tabular figures face |
| `--fs`, `--fs-sm`, `--fs-lg`, `--lh` | type sizes and line height |
| `--radius`, `--radius-sm`, `--gap`, `--pad`, `--tile-w`, `--header-h`, `--border` | geometry |
| `--anim`, `--anim-take` | how long things move for. Set both to `0s` for a still interface |
| `--shadow` | the shadow under a modal and a menu |

## The one rule

Red means going out. `--live` is spent on the tally, the on air tile's frame,
the ad pill and the top of a meter, and on nothing else. If your theme uses red
for errors as well, an operator glancing at the screen cannot tell a camera that
failed from a camera that is live. Use `--bad` for faults and keep it visibly
duller.

## Contrast

`high-contrast.css` is the floor, not the ceiling: it holds every text pair
above 7:1. If you are making a theme for a room with daylight in it, check
`--dim` against `--panel` and `--faint` against `--bg`, which are the two pairs
that go first.

A theme may also change `--border` (to `2px`, say) and the geometry properties.
`high-contrast.css` does both, which is the example to copy.

## Following the operating system

`system.css` redefines the light palette inside
`@media (prefers-color-scheme: light)` and leaves the dark defaults from
`base.css` alone. Copy that shape if you want a theme with two faces.

## Testing

Open `/test/` in the browser you care about with your theme selected. The tests
do not check colours, but the page they draw shows every component on one screen,
which is the fastest way to spot a property you forgot.
