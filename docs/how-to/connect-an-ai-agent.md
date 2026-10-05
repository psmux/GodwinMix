# Connect an AI agent, and have it make your graphics

Hook a coding agent up to GodwinMix and ask it, in plain words, for what you
want on screen: a lower third in your colours, a score bug, a title card, a
new background. It designs the graphic, puts it in the mixer, looks at the
result and changes the words on air when you ask.

This works with Claude Code, opencode, pi, Codex, Gemini CLI, Cursor and any
other agent that speaks MCP or can run a command.

## The quickest way: from the app

1. Open GodwinMix.
2. **Help > Connect an AI agent**.
3. Pick your agent and run the lines it shows, or paste them where it says.
   They name the mixer by its full path on this computer and need no address
   and no password: while GodwinMix is open, the agent finds it by itself.
4. Start your agent and ask for something:

   > Make a lower third for Ada Lovelace, Analyst, Engine Research, in our
   > green #0b6e3d, and put it on the studio scene. Don't show it yet.

The lines for each agent are below too, for a mixer on a server or for doing
it by hand. On Windows the mixer is `C:\Program Files\GodwinMix\godwinmix.exe`;
on macOS `/Applications/GodwinMix.app/Contents/MacOS/godwinmix`; from a Linux
package or a checkout, `godwinmix`. Write `godwinmix` below as that path.

## Claude Code

```sh
claude mcp add godwinmix -- godwinmix mcp
godwinmix skill install --for claude
```

## opencode

In `opencode.json`, in your project or in `~/.config/opencode/`:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "godwinmix": { "type": "local", "command": ["godwinmix", "mcp"], "enabled": true }
  }
}
```

```sh
godwinmix skill install --for opencode
```

## pi

pi has no MCP, on purpose: it reads skills and runs commands. GodwinMix gives
it both. The skills go where pi reads them, `~/.agents/skills`:

```sh
godwinmix skill install --for pi
```

and every tool a skill names runs as a command, with its arguments as JSON:

```sh
godwinmix tool list
godwinmix tool list_templates
godwinmix tool add_source '{"name": "lower", "uri": "template:news-lower-third", "params": {"fields": {"name": "Ada Lovelace"}}}'
```

That is the same tool table the MCP server serves, so nothing differs but the
way in. Tell pi once that "the GodwinMix tools run as `godwinmix tool NAME
JSON`" if it does not read that from the skill.

## Codex

In `~/.codex/config.toml`:

```toml
[mcp_servers.godwinmix]
command = "godwinmix"
args = ["mcp"]
```

```sh
godwinmix skill install --for codex
```

## Gemini CLI

In `~/.gemini/settings.json`:

```json
{ "mcpServers": { "godwinmix": { "command": "godwinmix", "args": ["mcp"] } } }
```

```sh
godwinmix skill install --for gemini
```

## Cursor, Claude Desktop and the rest

Any MCP client over stdio takes the same entry:

```json
{ "mcpServers": { "godwinmix": { "command": "godwinmix", "args": ["mcp"] } } }
```

`godwinmix skill install --for claude --print` lists the skill files; point
the client's skills or rules at them, or paste them in.

## A mixer on another machine

The agent's computer needs `godwinmix` too (any release archive). Give it the
mixer's address and token in the environment, or as `--url` and `--token`:

```sh
export GODWINMIX_URL=http://studio-pc:8080
export GODWINMIX_TOKEN=the-mixers-token
```

The desktop app keeps its token in a file named `core-token` in its data
folder: `%APPDATA%\mix.godwin.desktop` on Windows, `~/Library/Application
Support/mix.godwin.desktop` on macOS, `~/.local/share/mix.godwin.desktop` on
Linux.

## What to ask for

The skills teach the agent the rest: the built in pack of graphics, how a
template is written, safe areas, putting a graphic on a scene with an enter
and an exit, looking at the result, and changing its words on air.

* "Put a lower third on the studio scene for Ada Lovelace, Analyst. Slide it
  in from the left."
* "A breaking news bar, red, label NEWS 24, headline: Storm warning for the
  coast tonight. Don't show it yet."
* "Our colours are green #0b6e3d and gold #f2c230. Make our own lower third
  from the pack one, with the logo from the media library, and use it for
  every guest."
* "Score bug for Arsenal against Chelsea, 2 to 1, 67 minutes."
* "Change the headline to Coast road closed at Fairlight."

### Assets the agent makes itself

Whatever the agent makes, a template, a background, a ticker, a web graphic,
a clip or a virtual set, it can save into the Graphics gallery with one call,
`save_graphic`, and look at it with `preview_graphic`. It then shows as a card
under **View > Graphics**, ready to put on air in two clicks. See
[build your own graphics gallery with an AI agent](build-a-graphics-gallery-with-ai.md).

An agent that can write files can make a whole new graphic: it writes an SVG
template with `{{fields}}` in it and saves it with `save_template`, which
refuses one that would not draw. See [write an SVG
template](write-an-svg-template.md).

For pictures, a logo, a backdrop, a background for a presenter, the agent
makes or fetches the file and puts it in the media library:

```sh
godwinmix ctl upload studio-backdrop.png
```

Then it uses it like any other file: a picture source, a template's image
field, or the background behind a presenter, with or without a green screen
([replace the background](replace-the-background.md)).

## Check it is connected

```sh
godwinmix tool agent_state
```

prints what is on air and every source. If it says it cannot reach the
mixer, open GodwinMix first, or set `GODWINMIX_URL` for a mixer elsewhere.

## See also

* [Design graphics with an AI agent](design-graphics-with-ai.md): what the
  agent does, call by call.
* [Build your own graphics gallery with an AI agent](build-a-graphics-gallery-with-ai.md):
  save, look, fix, and put on air from the gallery.
* [Use the mixer from an AI agent](use-with-an-ai-agent.md): profiles,
  headends, the rules that refuse a take.
* [The operator playbook](../agents.md).
