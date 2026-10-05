# Connect an AI agent, and have it make your graphics

Hook a coding agent up to GodwinMix and ask it, in plain words, for what you
want on screen: a lower third in your colours, a ticker, a studio set, a cut
to the wide shot. It uses the mixer's own tools, looks at the result and
changes the words on air when you ask.

What is below was run on Windows 11 with Claude Code 2.1, opencode 1.18 and
pi 0.73, against a real mixer; what was checked on macOS and Linux, and how,
is said where it matters.

## The quickest way: Set up, in the app

1. Open GodwinMix.
2. **Help > Connect an AI agent**. The agents found on this computer are first
   and say "(installed)".
3. Pick yours and press **Set up**. The dialog lists every file it will write:
   the agent's MCP config, with one entry named `godwinmix`, and the three
   GodwinMix skills. **For a project folder** puts them in one project instead
   of your home folder.
4. Press **Write these files**. Anything else in those files is kept, and a
   file that changes is copied aside first as `<file>.before-godwinmix`.
5. Start the agent the way the dialog says, and paste one of the starter
   prompts it shows.

The entry runs the mixer's own executable as `godwinmix mcp`, with no address
and no password in it: while GodwinMix is open on this computer, the agent
finds it by itself.

## The same from a terminal

```sh
godwinmix agent tools                  # which agents are on this computer
godwinmix agent setup claude --dry-run # what it would write, writing nothing
godwinmix agent setup claude           # or opencode, pi, codex, gemini, cursor, vscode
godwinmix agent setup opencode --project .
```

`godwinmix` is the mixer's executable. An installed app is not on PATH, so
write it out in full:

| System | The executable |
|---|---|
| Windows | `& "C:\Program Files\GodwinMix\godwinmix.exe"` in PowerShell, `"C:\Program Files\GodwinMix\godwinmix.exe"` in cmd or Git Bash |
| macOS | `/Applications/GodwinMix.app/Contents/MacOS/godwinmix` |
| Linux | `godwinmix` from the .deb; from the AppImage, the path you extracted it to; from a checkout, `target/release/gmx` |

Where every tool keeps its files on each system is in the [agent setup
reference](../reference/agent-setup.md).

## By hand, per agent

Write `godwinmix` below as the full path from the table above.

### Claude Code

```sh
claude mcp add --scope user godwinmix -- godwinmix mcp
godwinmix skill install --for claude
```

For one project, `--scope project` writes `.mcp.json` in the folder and
`skill install --for claude --project` writes `.claude/skills`. Claude Code
asks once whether to trust a project's MCP server; in `claude -p` there is no
prompt, so pass `--mcp-config .mcp.json` instead. The name comes before `-e`:
`claude mcp add godwinmix -e GODWINMIX_URL=... -- godwinmix mcp`.

### opencode

In `opencode.json`, in your project or in `~/.config/opencode/` (the same
place on Windows, macOS and Linux; opencode reads `opencode.json` and
`opencode.jsonc` side by side):

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
opencode mcp list                      # godwinmix connected
```

Set up also adds a file of a few lines, `godwinmix.md`, to the config's
`instructions`. Without it a free model asked "what is on air?" did not
connect the question to the mixer.

### pi

pi has no MCP, on purpose: it reads skills and runs commands. The skills go
where pi reads them, `~/.agents/skills` (or `.agents/skills` in a project):

```sh
godwinmix skill install --for pi
```

and every tool runs as a command, with its arguments as JSON:

```sh
godwinmix tool agent_state
godwinmix tool add_source '{"id": "lower", "uri": "template:news-lower-third", "params": {"fields": {"name": "Ana Silva"}}}'
```

pi runs commands in bash, Git Bash on Windows, so the single quotes work
everywhere. When the mixer is not on PATH the installed skills say so in
their first line and name the full path to run instead.

### Codex

In `~/.codex/config.toml`:

```toml
[mcp_servers.godwinmix]
command = "godwinmix"
args = ["mcp"]
```

```sh
godwinmix skill install --for codex
```

### Gemini CLI

In `~/.gemini/settings.json`:

```json
{ "mcpServers": { "godwinmix": { "command": "godwinmix", "args": ["mcp"] } } }
```

```sh
godwinmix skill install --for gemini
```

### Cursor, VS Code and any other client

Cursor reads `~/.cursor/mcp.json` or `.cursor/mcp.json` in a project:

```json
{ "mcpServers": { "godwinmix": { "command": "godwinmix", "args": ["mcp"] } } }
```

VS Code reads `.vscode/mcp.json`, or the user one from **MCP: Open User
Configuration**, with `servers` rather than `mcpServers`:

```json
{ "servers": { "godwinmix": { "type": "stdio", "command": "godwinmix", "args": ["mcp"] } } }
```

Any client that speaks MCP over stdio takes the Cursor shape.
`godwinmix agent setup other` prints it with the full path filled in.

## A mixer on another machine, or one you started yourself

`godwinmix mcp` and `godwinmix tool` look for a mixer in this order: `--url`
and `--token`, then `GODWINMIX_URL` and `GODWINMIX_TOKEN`, then the desktop
app's own mixer on this computer, then `http://127.0.0.1:8080`. The desktop
app writes its port and token to `local-core.port` and `core-token` in its
data folder:

| System | Data folder |
|---|---|
| Windows | `%APPDATA%\mix.godwin.desktop` |
| macOS | `~/Library/Application Support/mix.godwin.desktop` |
| Linux | `$XDG_DATA_HOME/mix.godwin.desktop`, or `~/.local/share/mix.godwin.desktop` |

For any other mixer, give the agent the address and the token. Set up does it
for you:

```sh
godwinmix agent setup claude --url http://studio-pc:8080 --token the-mixers-token
```

which puts both in the entry's environment. Keep that file to yourself: the
token is in it.

## Check it is connected

```sh
godwinmix tool agent_state
```

prints what is on air and every source. If it cannot reach the mixer, open
GodwinMix first, or set `GODWINMIX_URL`.

## Prompts that work

PROMPTS_TABLE

## When it goes wrong

* **The agent says it cannot run a tool it found.** That was before
  `call_tool`: Claude Code and opencode only let a model call what is in its
  list. Update the mixer.
* **opencode says `godwinmix failed`, "Failed to get tools".** A tool schema
  its MCP client could not read; fixed in this release. `opencode mcp list`
  shows the state.
* **"command not found: godwinmix" from pi or a shell.** The app is not on
  PATH. Run Set up again, or `skill install`, from the app's own executable:
  the skills then name its full path.
* **The agent asks which station you mean.** It did not connect "on air" to
  the mixer. Name it once ("in GodwinMix, ..."), or run Set up, which tells
  opencode in its instructions.

## See also

* [Design graphics with an AI agent](design-graphics-with-ai.md): what the
  agent does, call by call.
* [Use the mixer from an AI agent](use-with-an-ai-agent.md): profiles,
  headends, the rules that refuse a take.
* [Agent setup reference](../reference/agent-setup.md).
* [The operator playbook](../agents.md).
