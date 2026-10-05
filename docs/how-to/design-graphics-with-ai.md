# Design graphics with an AI agent

Ask an agent for "a red breaking news bar that says NEWS 24" and have it on
air a minute later, designed, branded and checked. The agent writes or picks
an [SVG template](write-an-svg-template.md), the mixer draws it with no
browser, and the agent looks at the result before it tells you it is done.

This works with Claude Code, opencode, pi, Codex, Gemini CLI, Cursor, VS
Code and any other agent that speaks MCP or runs commands. A free model in
opencode put a pack lower third on air, changed its words and cut between
cameras; designing a new template or a studio set from nothing went better
with a stronger one.

To keep what the agent makes, and to make backgrounds, tickers, web graphics,
clips and virtual sets too, have it save each into the Graphics gallery: see
[build your own graphics gallery with an AI agent](build-a-graphics-gallery-with-ai.md).

## 1. Connect the agent and give it the skill

In GodwinMix, **Help > Connect an AI agent**, pick your agent and press **Set
up**. It shows the files it will write, and writes them when you say so: the
agent's MCP entry for this mixer and the three skills. From a terminal it is
one line:

```sh
godwinmix agent setup claude          # or opencode, pi, codex, gemini, cursor, vscode
```

Every agent, every system and a mixer on another machine are in [connect an
AI agent](connect-an-ai-agent.md).

`godwinmix-design` is the skill that teaches graphics: the pack and its
fields, how a template is written, safe areas, placing one on the scene that
is on air with an enter and an exit, tickers, backgrounds, the virtual set,
and how to look at the result. The tools it names (`list_templates`,
`save_template`, `add_source`, `add_scene_item`, `set_scene_item`,
`create_scene_from`, `snapshot`) are in the standard tool list; the rest run
through `call_tool`.

## 2. Ask for what you want

Say what it is for, what it says and what it looks like. These were run
against a real mixer with Claude Code (Opus) and opencode (a free model); the
results are in [connect an AI agent](connect-an-ai-agent.md#prompts-that-work).

* "Add a lower third that says Ana Silva, Producer, in my brand blue, and put
  it on air."
* "Make an animated news ticker with these three headlines: ..."
* "Design a background for my presenter."
* "Make me a modern news studio set and put me in it, no green screen."

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

For a pack graphic it makes five or six calls:

1. `agent_state`, to see what is on air. When that is a camera rather than a
   scene, `create_scene_from {"sources": ["cam1"], "name": "Live"}` and
   `take {"scene": "Live"}` first; the picture does not change.
2. `list_templates`, once, to see what there is and what each field is called.
3. `add_source` with `uri: "template:breaking-news"` and its words in
   `params.fields`.
4. `add_scene_item` on that scene. With no `transform` a template covers the
   whole canvas, which is where it was designed to sit. `visible: false` with
   an `enter` and an `exit` places it hidden.
5. `set_scene_item` with `visible: true` when you say go, then a look with
   `snapshot` of the programme.

A ticker is a `ticker:` source with `params.items`, placed as a bar along the
bottom. A background is a full screen template saved with `save_template`. A
virtual set is `create_scene_from` with layout `virtual-set`: the background,
the camera, and `settings.screen: "none"` when there is no green screen, which
cuts the person out with a model. There is no separate virtual set feature.

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

* The graphic sits in half the picture. It is not a template (a picture, a
  web page) and was placed with no `transform`, so it took the next grid
  cell. Tell the agent to place it at 0, 0 with the canvas size.
* "There is no scene called studio". What is on air is a camera, not a scene;
  the refusal says to make one with `create_scene_from`, and agents do.
* The set has no presenter in it, with "ONNX Runtime is not on this machine".
  The person cutout needs ONNX Runtime, which the desktop installers carry; a
  mixer built from source needs it installed or `ORT_DYLIB_PATH` set.
* The words are tiny. They were shrunk to fit a long headline. Ask for a
  shorter one, or for the two line strap.
* The agent reaches for OGraf for a lower third. Point it at the skill: OGraf
  is for graphics that move inside themselves, and runs a browser per graphic.
