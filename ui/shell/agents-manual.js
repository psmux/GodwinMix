// The lines to paste into each agent tool by hand, written for this
// machine: the mixer's own executable by its full path (from
// `core.info.executable`), because an installed app is not on anybody's
// PATH, and no address and no token, because `godwinmix mcp` and `godwinmix
// tool` find the desktop app's mixer on the same machine by themselves.

/** A path, quoted for a shell when it has a space in it. */
export function quoted(path) {
  return /[\s"]/.test(path) ? `"${path.replace(/"/g, '\\"')}"` : path;
}

/** Every agent's setup, for the executable at `exe`. */
export function setups(exe) {
  const q = quoted(exe);
  const json = (v) => JSON.stringify(v, null, 2);
  return [
    {
      id: "claude",
      name: "Claude Code",
      steps: [
        ["Connect the mixer", `claude mcp add godwinmix -- ${q} mcp`],
        ["Teach it GodwinMix", `${q} skill install --for claude`],
      ],
    },
    {
      id: "opencode",
      name: "opencode",
      steps: [
        ["Add to opencode.json, in your project or in ~/.config/opencode", json({ $schema: "https://opencode.ai/config.json", mcp: { godwinmix: { type: "local", command: [exe, "mcp"], enabled: true } } })],
        ["Teach it GodwinMix", `${q} skill install --for opencode`],
      ],
    },
    {
      id: "pi",
      name: "pi",
      note: "pi has no MCP, by design: it reads skills and runs commands. The skills tell it to run each tool with the line below.",
      steps: [
        ["Teach it GodwinMix", `${q} skill install --for pi`],
        ["How it runs a tool", `${q} tool list\n${q} tool add_source '{"name": "news", "uri": "template:breaking-news"}'`],
      ],
    },
    {
      id: "codex",
      name: "Codex",
      steps: [
        ["Add to ~/.codex/config.toml", `[mcp_servers.godwinmix]\ncommand = ${JSON.stringify(exe)}\nargs = ["mcp"]`],
        ["Teach it GodwinMix", `${q} skill install --for codex`],
      ],
    },
    {
      id: "gemini",
      name: "Gemini CLI",
      steps: [
        ["Add to ~/.gemini/settings.json", json({ mcpServers: { godwinmix: { command: exe, args: ["mcp"] } } })],
        ["Teach it GodwinMix", `${q} skill install --for gemini`],
      ],
    },
    {
      id: "cursor",
      name: "Cursor",
      steps: [["Add to ~/.cursor/mcp.json, or .cursor/mcp.json in a project", json({ mcpServers: { godwinmix: { command: exe, args: ["mcp"] } } })]],
    },
    {
      id: "vscode",
      name: "VS Code",
      steps: [["Add to .vscode/mcp.json in a project, or MCP: Open User Configuration", json({ servers: { godwinmix: { type: "stdio", command: exe, args: ["mcp"] } } })]],
    },
    {
      id: "other",
      name: "Any MCP client",
      note: "Any client that speaks MCP over stdio takes this shape. Point its skills folder at the files `skill install --print` lists.",
      steps: [["The MCP server entry", json({ mcpServers: { godwinmix: { command: exe, args: ["mcp"] } } })]],
    },
  ];
}
