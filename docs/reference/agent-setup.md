# Agent setup

`agent.setup` and `agent.tools`, the methods behind the Set up buttons in
Help > Connect an AI agent, and `gmx agent setup`, the same from a terminal.
Both are admin scope: they read and write files in the home folder of the
user the mixer runs as.

## `agent.tools`

The agent tools this mixer knows how to set up, the installed ones first.

```json
[{"tool": "opencode", "name": "opencode", "installed": true, "found": "C:\\Users\\ana\\.bun\\bin\\opencode.exe"},
 {"tool": "codex", "name": "Codex", "installed": true, "found": "C:\\Users\\ana\\.codex"},
 {"tool": "gemini", "name": "Gemini CLI", "installed": false}]
```

A tool counts as installed when its command is on PATH (on Windows with each
`PATHEXT` ending) or the folder it makes on first run exists: `~/.claude`,
`~/.config/opencode`, `~/.pi`, `~/.codex`, `~/.gemini`, `~/.cursor`, or VS
Code's `Code` folder in the system's application data folder.

## `agent.setup`

| Param | Type | Meaning |
|---|---|---|
| `tool` | string | `claude`, `opencode`, `pi`, `codex`, `gemini`, `cursor`, `vscode` or `other` |
| `scope` | string | `user` (the default), into the home folder, or `project`, into `dir` |
| `dir` | string | the project folder, an absolute path; needed with `scope: project` |
| `env` | object | environment for `godwinmix mcp`, written into the entry as given, such as `GODWINMIX_URL` for a mixer that is not the desktop app's |
| `dry_run` | boolean | answer every file it would write and write nothing |

It is marked destructive, because it changes another program's
configuration: it takes `dry_run`, and on a token whose policy is
`confirm = "required"` it wants a confirmation first.

The answer:

```json
{
  "tool": "opencode", "name": "opencode", "scope": "user", "applied": true,
  "writes": [
    {"path": "/home/ana/.config/opencode/opencode.json", "action": "merge", "what": "the godwinmix MCP server",
     "backup": "/home/ana/.config/opencode/opencode.json.before-godwinmix"},
    {"path": "/home/ana/.config/opencode/godwinmix.md", "action": "create", "what": "what GodwinMix is, read every session"},
    {"path": "/home/ana/.config/opencode/skills/godwinmix-operate/SKILL.md", "action": "create", "what": "the godwinmix-operate skill"}
  ],
  "start": "In a terminal, run `opencode`, then ask it something.",
  "prompt": "Make a lower third for Ana Silva, Producer, in blue, and put it on air."
}
```

`action` is `create` (there was no file), `merge` (the entry goes in beside
what the file has), `update` (the file had a `godwinmix` entry that differed)
or `unchanged`. A dry run answers the same `writes` with `would_change` and
`diff`, and `applied: false`.

What it promises:

* Only the entry named `godwinmix` changes. Every other key in a JSON file is
  kept; Codex's TOML keeps its comments and order too.
* A config file it changes is copied aside first, as
  `<file>.before-godwinmix` (then `.1`, `.2`, so the first original is never
  overwritten). Skill files are the mixer's own and are replaced in place.
* A file it cannot read as what it should be, such as JSON with comments, is
  refused with the entry to paste by hand, and nothing is written.
* Running it twice changes nothing the second time.

## Where each tool reads

`~` is the home folder: `%USERPROFILE%` on Windows, `$HOME` elsewhere. The
same paths on Windows, macOS and Linux except where the table says.

| Tool | MCP config, user | MCP config, project | Skills, user | Skills, project |
|---|---|---|---|---|
| Claude Code | `~/.claude.json`, `mcpServers` | `.mcp.json`, `mcpServers` | `~/.claude/skills` | `.claude/skills` |
| opencode | `~/.config/opencode/opencode.json`, `mcp` | `opencode.json`, `mcp` | `~/.config/opencode/skills` | `.opencode/skills` |
| pi | none: pi has no MCP | none | `~/.agents/skills` | `.agents/skills` |
| Codex | `~/.codex/config.toml`, `[mcp_servers]` | `.codex/config.toml` | `~/.codex/skills` | `.codex/skills` |
| Gemini CLI | `~/.gemini/settings.json`, `mcpServers` | `.gemini/settings.json` | `~/.gemini/skills` | `.gemini/skills` |
| Cursor | `~/.cursor/mcp.json`, `mcpServers` | `.cursor/mcp.json` | `~/.cursor/skills` | `.cursor/skills` |
| VS Code | user `mcp.json`, `servers` (below) | `.vscode/mcp.json`, `servers` | `~/.copilot/skills` | `.github/skills` |

VS Code's user folder is `%APPDATA%\Code\User` on Windows, `~/Library/Application
Support/Code/User` on macOS and `$XDG_CONFIG_HOME/Code/User` (or
`~/.config/Code/User`) on Linux. opencode uses `~/.config/opencode` on every
system, Windows included, and reads `opencode.json` and `opencode.jsonc` side
by side, so a setup never has to touch a `.jsonc` you wrote.

The entry each tool gets, for a mixer at `/opt/godwinmix/godwinmix`:

| Tool | Entry |
|---|---|
| Claude Code, VS Code | `{"type": "stdio", "command": "/opt/godwinmix/godwinmix", "args": ["mcp"]}` |
| opencode | `{"type": "local", "command": ["/opt/godwinmix/godwinmix", "mcp"], "enabled": true}` |
| Codex | `command = "/opt/godwinmix/godwinmix"`, `args = ["mcp"]` |
| Gemini CLI, Cursor, any other | `{"command": "/opt/godwinmix/godwinmix", "args": ["mcp"]}` |

`env` goes in as `env`, or `environment` for opencode.

## Two extras

opencode also gets `godwinmix.md`, a few lines saying that "on air", a camera
or a lower third mean GodwinMix and to start with `agent_state`, named in the
config's `instructions` so every session reads it. A free model asked "what is
on air right now?" checked the clock and asked which country without it.

When `godwinmix` on PATH is not the mixer that wrote the skills (an installed
app is on nobody's PATH), each skill gets one line after its frontmatter
naming the executable to run instead, quoted, with the PowerShell form too.
`gmx skill install` adds the same line.

## Not covered

The Claude Code user config is a file Claude Code itself rewrites; quit it
before a setup for the user, or use a project. Cursor's and VS Code's skill
folders are the ones their documentation names in October 2026 and were not
run with a model here.
