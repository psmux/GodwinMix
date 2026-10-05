// "Make one with an AI agent": pick a kind, say a word about the show, and
// copy a prompt that tells any agent exactly which tools to call. The agent
// saves into this gallery, so the new graphic appears here by itself.
//
// Nothing here talks to a model. If no agent is set up yet, Help > Connect
// an AI agent is one button away.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast } from "../../shell/toast.js";
import { KINDS, promptFor } from "./prompts.js";

/** The dialog, opened on `kindId`. */
export function makeWithAgent(client, kindId) {
  let kind = kindId || KINDS[0].id;
  const about = el("input", { type: "text", placeholder: "Your show, in a few words: a red and white evening news, a church livestream...", "aria-label": "About the show" });
  const text = el("textarea.gx-prompt", { rows: 9, readonly: true, "aria-label": "The prompt" });
  const fill = () => (text.value = promptFor(kind, about.value));
  about.oninput = fill;
  const chips = el("div.row.wrap.gx-kinds", {}, KINDS.map((k) => {
    const b = el("button.btn", { text: k.label, "aria-pressed": String(k.id === kind) });
    b.onclick = () => {
      kind = k.id;
      for (const c of chips.children) c.setAttribute("aria-pressed", String(c === b));
      fill();
    };
    return b;
  }));
  const copy = el("button.btn.primary", { text: "Copy the prompt" });
  copy.onclick = async () => {
    fill();
    const ok = await navigator.clipboard?.writeText(text.value).then(() => true, () => false);
    if (!ok) {
      text.select();
      document.execCommand?.("copy");
    }
    toast({ text: "Copied. Paste it into your AI agent; what it saves appears in Graphics." });
  };
  const setup = el("button.btn", { text: "Set up an AI agent", onclick: () => import("../../shell/agents.js").then((m) => m.openAgents(client)) });
  fill();
  return modal({
    title: "Make a graphic with an AI agent",
    body: el("div.col", {}, [
      chips,
      about,
      text,
      el("p.sm.dim", { text: "Works with Claude Code, opencode, pi and any agent with the GodwinMix tools or a shell. No agent connected yet? Set one up first; it takes a minute." }),
    ]),
    footer: [setup, el("span.grow"), copy],
  });
}
