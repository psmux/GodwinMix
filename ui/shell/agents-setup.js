// The Set up button in Help > Connect an AI agent, for one tool.
//
// One click shows exactly which files would be written (`agent.setup` with
// `dry_run`), a second writes them, and the answer says how to start the tool
// and what to ask it first. Nothing is written before the person has seen
// the list. For a project rather than for this user, a folder on the mixer's
// machine is chosen with the folder picker.

import { el } from "./dom.js";

/** A short word for what a write does to a file. */
export function actionText(action) {
  return { create: "new", merge: "add to", update: "update", unchanged: "already done" }[action] || action;
}

/** The list of files a plan writes, as rows a person can read. */
export function writeRows(writes) {
  return (writes || []).map((w) =>
    el("div.row", {}, [
      el("span.sm.dim", { text: actionText(w.action), style: { minWidth: "7em" } }),
      el("code.sm.grow", { text: w.path, style: { overflowWrap: "anywhere" } }),
      el("span.sm.dim", { text: w.what }),
    ]),
  );
}

/**
 * @param {object} client  the page's client
 * @param {{tool: string, name: string}} tool
 * @param {(label: string, text: string) => Node} copyable
 */
export function setupPane(client, tool, copyable) {
  const pane = el("div.col");
  const status = el("div.col", { role: "status" });
  let scope = "user";
  let dir = null;
  const where = el("span.sm.dim", { text: "for you, in your home folder" });
  const forMe = el("button.btn.sm", { text: "For me", onclick: () => pick("user") });
  const forProject = el("button.btn.sm", { text: "For a project folder", onclick: () => pick("project") });
  const go = el("button.btn.primary", { text: `Set up ${tool.name}`, onclick: () => preview() });

  async function pick(which) {
    if (which === "project") {
      const { pickFolder } = await import("./folder-picker.js");
      const chosen = await pickFolder(client, { title: "The project folder" });
      if (!chosen) return;
      dir = chosen;
      where.textContent = `for the project in ${chosen}`;
    } else {
      dir = null;
      where.textContent = "for you, in your home folder";
    }
    scope = which;
    forMe.classList.toggle("primary", scope === "user");
    forProject.classList.toggle("primary", scope === "project");
  }

  const params = () => (scope === "project" ? { tool: tool.tool, scope, dir } : { tool: tool.tool, scope });

  async function preview() {
    status.replaceChildren(el("p.sm.dim", { text: "Working out what to write..." }));
    try {
      const plan = await client.call("agent.setup", { ...params(), dry_run: true });
      const nothing = !plan.would_change;
      const write = el("button.btn.primary", { text: nothing ? "Nothing to change" : "Write these files", disabled: nothing, onclick: () => apply() });
      status.replaceChildren(
        el("p", { text: `${tool.name} needs these files. Other settings in them are kept, and a changed file is copied aside first.` }),
        ...writeRows(plan.writes),
        ...(plan.notes || []).map((n) => el("p.sm", { text: n })),
        plan.entry ? copyable("Paste this into your client's MCP settings", JSON.stringify(plan.entry, null, 2)) : null,
        el("div.row", {}, [write]),
      );
    } catch (e) {
      status.replaceChildren(el("p.sm", { text: `Could not set up ${tool.name}: ${e.message}` }));
    }
  }

  async function apply() {
    try {
      const done = await client.call("agent.setup", params());
      status.replaceChildren(
        el("p", { text: `${tool.name} is set up.` }),
        ...writeRows(done.writes),
        ...done.writes.filter((w) => w.backup).map((w) => el("p.sm.dim", { text: `The old ${w.path} is kept as ${w.backup}.` })),
        el("p", { text: done.start }),
        copyable("Then ask it", done.prompt),
      );
    } catch (e) {
      status.replaceChildren(el("p.sm", { text: `Nothing was finished: ${e.message}` }));
    }
  }

  pane.append(el("div.row.wrap", {}, [forMe, forProject, where]), el("div.row", {}, [go]), status);
  forMe.classList.add("primary");
  return pane;
}
