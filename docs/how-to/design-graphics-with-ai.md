# Design graphics with an AI agent

Ask an agent for "a red breaking news bar that says NEWS 24" and have it on
air a minute later, designed, branded and checked. The agent writes or picks
an [SVG template](write-an-svg-template.md), the mixer draws it with no
browser, and the agent looks at the result before it tells you it is done.

This works with Claude Code, opencode, pi, Codex, Gemini CLI, Cursor and
any other agent that speaks MCP or runs commands. A small model does the pack
and the field changes well; designing a new template from nothing wants a
stronger one.

To keep what the agent makes, and to make backgrounds, tickers, web graphics,
clips and virtual sets too, have it save each into the Graphics gallery: see
[build your own graphics gallery with an AI agent](build-a-graphics-gallery-with-ai.md).

## 1. Connect the agent and give it the skill

In GodwinMix, **Help > Connect an AI agent** shows the lines for your agent,
written for your computer: run them and you are done. Every agent, and a mixer
on another machine, is in [connect an AI agent](connect-an-ai-agent.md). For
Claude Code it is:

```sh
claude mcp add godwinmix -- godwinmix mcp
```

Then install the skills. `godwinmix-design` is the one that teaches graphics:
when to use a text source, a template or OGraf, the pack and its fields, how a
template is written, safe areas, how to put one on a scene with an enter and an
exit, and how to look at it.

```sh
gmx skill install --for claude          # or opencode, pi, codex, gemini
```

The template tools are behind `search_tools`, so they cost nothing in the tool
list until the agent needs them. The skill names them, so the agent calls them
directly.

## 2. Ask for what you want

Say what it is for, what it says and what it looks like. Some requests that
work:

* "Put a lower third on the studio scene for Ada Lovelace, Analyst, Engine
  Research. Slide it in from the left."
* "A breaking news bar, red, label NEWS 24, headline: Storm warning for the
  coast tonight. Don't show it yet."
* "Score bug for Arsenal against Chelsea, 2 to 1, 67 minutes, second half. Keep
  the clock running once a second."
* "Our colours are green #0b6e3d and gold #f2c230. Make our own lower third
  from the pack one with the logo from the media library, and use it for the
  next guest."

## 3. What the agent does

For a pack graphic it makes five calls:

1. `list_templates`, once, to see what there is and what each field is called.
2. `add_source` with `uri: "template:breaking-news"` and its words in
   `params.fields`.
3. `add_scene_item` with the item over the whole canvas, `visible: false`, and
   an `enter` and an `exit`.
4. A look: `arm_preview` and `preview_frame`, which answers with the picture
   itself, or `snapshot` of the programme once it is shown.
5. `set_scene_item` with `visible: true` when you say go.

After that, each change of words is one `set_source` with just the fields that
changed. It reaches the screen on the next frame.

For a design of its own it reads a pack template with `get_template`, changes
it, checks it with `save_template` (which refuses an SVG that would not draw),
and looks at it the same way. If it is wrong it saves again with
`replace: true`, and the graphic on screen is drawn again with the fix.

## 4. Change it on air

Tell the agent and it changes the field:

> Change the headline to "Coast road closed at Fairlight".

```
set_source {"id": "breaking", "params": {"fields": {"headline": "Coast road closed at Fairlight"}}}
```

A person can do the same from the graphic's settings in the web UI, from
`gmx ctl source set breaking --param fields.headline="..."`, or from a data
feed bound to `params.fields.headline`. They all write the same param.

To take it off, ask for that; the agent sets `visible` to false and the item
leaves the way its `exit` says.

## What it costs

A graphic held on screen costs the blend of its panel and nothing else. A
field changed once a second is one small render a second on the overlay
worker. The numbers for a lower third and a breaking news bar on a 720p30
programme are in the [reference](../reference/graphic-templates.md#what-it-costs).

What the agent spends is mostly looking. A `preview_frame` at 1280 wide is
about 1,200 tokens, and that is the size for reading the words in a lower
third. 320 wide, about 84 tokens, is enough to see where things are.

## When it goes wrong

* The agent puts the graphic in a grid cell instead of over the canvas. It
  left out `transform`. Tell it to place the item at 0, 0 with the canvas size
  from `core_info`; the skill says so too.
* The words are tiny. They were shrunk to fit a long headline. Ask for a
  shorter one, or for the two line strap.
* The agent reaches for OGraf for a lower third. Point it at the skill: OGraf
  is for graphics that move inside themselves, and runs a browser per graphic.
