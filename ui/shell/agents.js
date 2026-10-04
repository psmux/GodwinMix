// Help > Connect an AI agent: the lines to paste into Claude Code, opencode,
// pi, Codex, Gemini CLI or any other MCP client, written for this machine.
//
// The commands name the mixer's own executable by its full path (from
// `core.info.executable`), because an installed app is not on anybody's PATH,
// and they carry no address and no token: `godwinmix mcp` and `godwinmix
// tool` find the desktop app's mixer on the same machine by themselves. For
// an agent on another machine the address and where to find the token are
// given instead. Loaded on first use.

import { el } from "./dom.js";
import { modal } from "./modal.js";
import { toast } from "./toast.js";

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
      id: "other",
      name: "Cursor, Claude Desktop, others",
      note: "Any client that speaks MCP over stdio takes this shape. Point its skills folder at the files `skill install --print` lists.",
      steps: [["The MCP server entry", json({ mcpServers: { godwinmix: { command: exe, args: ["mcp"] } } })]],
    },
  ];
}

function copyable(label, text) {
  const pre = el("pre.code", { text });
  const copy = el("button.btn.sm", {
    text: "Copy",
    onclick: async () => {
      try {
        await navigator.clipboard.writeText(text);
        toast({ text: "Copied." });
      } catch {
        toast({ text: "Select the text and copy it; this page may not use the clipboard." });
      }
    },
  });
  return el("div.col", {}, [el("div.row", {}, [el("span.sm.grow", { text: label }), copy]), pre]);
}

/** The dialog. */
export async function openAgents(client) {
  const info = await client.call("core.info", {}).catch(() => ({}));
  const exe = info.executable || "godwinmix";
  const all = setups(exe);
  const body = el("div.col");
  const tabs = el("div.row.wrap");
  const pane = el("div.col");
  const show = (s) => {
    pane.replaceChildren(...[s.note ? el("p.sm.dim", { text: s.note }) : null, ...s.steps.map(([l, t]) => copyable(l, t))].filter(Boolean));
    for (const b of tabs.children) b.classList.toggle("primary", b.dataset.id === s.id);
  };
  for (const s of all) {
    const b = el("button.btn.sm", { text: s.name, onclick: () => show(s) });
    b.dataset.id = s.id;
    tabs.appendChild(b);
  }
  body.append(
    el("p", { text: "Let an AI agent run this mixer and design its graphics: lower thirds, score bugs, title cards in your colours. Pick your agent, run the lines, then ask it in plain words, for example \"make a lower third for Ada Lovelace, Analyst, in our green, and put it on the studio scene\"." }),
    tabs,
    pane,
    el("p.sm.dim", { text: `The commands find this mixer by themselves while GodwinMix is open on this machine. For an agent on another machine, set GODWINMIX_URL to ${location.origin} and GODWINMIX_TOKEN to the mixer's token (the desktop app keeps it in a file named core-token in its data folder).` }),
    el("a", { href: "https://github.com/psmux/GodwinMix/blob/main/docs/how-to/design-graphics-with-ai.md", target: "_blank", rel: "noopener", text: "More: designing graphics with an AI agent" }),
  );
  show(all[0]);
  return modal({ title: "Connect an AI agent", body, wide: true });
}
