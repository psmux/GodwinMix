// Help > Connect an AI agent.
//
// Each agent tool gets a Set up button (agents-setup.js), which writes its
// MCP entry and the skills through `agent.setup` after showing the files; the
// same as a command, `godwinmix agent setup <tool>`; and the lines to paste
// by hand (agents-manual.js). The tools found on the mixer's machine
// (`agent.tools`) come first. Loaded on first use.

import { el } from "./dom.js";
import { modal } from "./modal.js";
import { toast } from "./toast.js";
import { setupPane } from "./agents-setup.js";
import { quoted, setups } from "./agents-manual.js";

export { quoted, setups };

/** Things to ask first, one click to copy each. */
export const STARTERS = [
  "Make me a lower third for Ana Silva, Producer, in blue, and put it on air.",
  "Build me a modern news studio set and put me in it, no green screen.",
  "Make an animated news ticker with these headlines: Storm closes coast road; Council approves new bridge.",
  "What is on air right now?",
];

/** The tools in the order to show them: installed first, as `agent.tools` answers. */
export function ordered(all, detected) {
  if (!Array.isArray(detected) || !detected.length) return all.map((s) => ({ ...s, installed: false }));
  const rank = new Map(detected.map((d, i) => [d.tool, i]));
  const found = new Map(detected.map((d) => [d.tool, d.installed]));
  return [...all]
    .sort((a, b) => (rank.get(a.id) ?? 99) - (rank.get(b.id) ?? 99))
    .map((s) => ({ ...s, installed: !!found.get(s.id) }));
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

/** One tool: the Set up button, the command that does the same, the lines by hand. */
function toolPane(client, s, exe) {
  const parts = [s.note ? el("p.sm.dim", { text: s.note }) : null];
  if (s.id !== "other") {
    parts.push(setupPane(client, { tool: s.id, name: s.name }, copyable));
    parts.push(copyable("Or from a terminal", `${quoted(exe)} agent setup ${s.id}`));
  }
  parts.push(el("details", {}, [el("summary.sm", { text: "Or by hand" }), ...s.steps.map(([l, t]) => copyable(l, t))]));
  return parts.filter(Boolean);
}

/** The dialog. */
export async function openAgents(client) {
  const info = await client.call("core.info", {}).catch(() => ({}));
  const detected = await client.call("agent.tools", {}).catch(() => []);
  const exe = info.executable || "godwinmix";
  const all = ordered(setups(exe), detected);
  const tabs = el("div.row.wrap");
  const pane = el("div.col");
  const show = (s) => {
    pane.replaceChildren(...toolPane(client, s, exe));
    for (const b of tabs.children) b.classList.toggle("primary", b.dataset.id === s.id);
  };
  for (const s of all) {
    const b = el("button.btn.sm", { text: s.installed ? `${s.name} (installed)` : s.name, onclick: () => show(s) });
    b.dataset.id = s.id;
    tabs.appendChild(b);
  }
  const body = el("div.col", {}, [
    el("p", { text: "Let an AI agent run this mixer and design its graphics. Pick your agent and press Set up, then start it and ask in plain words." }),
    tabs,
    pane,
    el("p.sm", { text: "Things to ask first:" }),
    ...STARTERS.map((t) => copyable("", t)),
    el("p.sm.dim", { text: `The agent finds this mixer by itself while GodwinMix is open on this machine. For an agent on another machine, set GODWINMIX_URL to ${location.origin} and GODWINMIX_TOKEN to the mixer's token (the desktop app keeps it in a file named core-token in its data folder).` }),
    el("a", { href: "https://github.com/psmux/GodwinMix/blob/main/docs/how-to/connect-an-ai-agent.md", target: "_blank", rel: "noopener", text: "More: connect an AI agent" }),
  ]);
  show(all[0]);
  return modal({ title: "Connect an AI agent", body, wide: true });
}

