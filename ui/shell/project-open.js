// Opening a project: what it would change, then the change.
//
// The mixer's own dry run is the summary, so what the dialog says is what
// the import will do, worked out by the code that will do it.

import { el, clear } from "./dom.js";
import { modal } from "./modal.js";
import { toast, errorText } from "./toast.js";
import { applyPage } from "./project.js";

const PARTS = [["setting", "Settings"], ["source", "Sources"], ["output", "Outputs"], ["channel", "Channels"], ["scene", "Scenes"], ["media", "Clips"]];
const WORDS = { add: "added", replace: "replaced", remove: "removed", keep: "kept", rename: "renamed", set: "changed", wait: "waiting", skip: "skipped", missing: "missing" };

export function review(client, file, fileName) {
  let mode = "replace";
  const body = el("div.col.project-review");
  const summary = el("div.col");
  const openButton = el("button.btn.primary", { text: "Open project", disabled: true });
  const choice = (value, label, hint) =>
    el("label.row", {}, [
      el("input", { type: "radio", name: "project-mode", value, checked: value === mode, onchange: () => { mode = value; check(); } }),
      el("span", {}, [el("strong", { text: label }), el("span.dim.sm", { text: " " + hint })]),
    ]);
  body.append(
    el("p", { text: `${file.name || fileName}${file.written_by ? `, saved by ${file.written_by}` : ""}.` }),
    choice("replace", "Replace this mixer's setup", "Its sources, outputs, channels and scenes become this mixer's."),
    choice("merge", "Add it beside what is here", "Nothing here is removed; anything with a name already taken is renamed."),
    summary
  );
  const dialog = modal({ title: "Open project", body, wide: true, footer: [el("button.btn", { text: "Cancel", onclick: () => dialog.close() }), openButton] });

  async function check() {
    openButton.disabled = true;
    clear(summary);
    summary.append(el("p.dim", { text: "Reading the file…" }));
    try {
      const plan = await client.call("project.import", { file, mode, dry_run: true });
      clear(summary);
      summary.append(...describe(plan));
      openButton.disabled = false;
    } catch (e) {
      clear(summary);
      summary.append(el("p", { role: "alert", text: errorText(e, "This project cannot be opened") }));
    }
  }

  openButton.onclick = async () => {
    openButton.disabled = true;
    openButton.textContent = "Opening…";
    try {
      const done = await client.call("project.import", { file, mode, dry_run: false });
      dialog.close();
      if (mode === "replace") await applyPage(done.page);
      finished(done);
    } catch (e) {
      openButton.textContent = "Open project";
      openButton.disabled = false;
      clear(summary);
      summary.append(el("p", { role: "alert", text: errorText(e, "Open project") }));
    }
  };
  check();
}

/** Counts per part, then the lists a person has to read. */
function describe(plan) {
  const out = [];
  const rows = [];
  for (const [part, title] of PARTS) {
    const mine = plan.changes.filter((c) => c.part === part && !(part === "setting" && c.id === "machine"));
    if (!mine.length) continue;
    const counts = {};
    for (const c of mine) counts[c.action] = (counts[c.action] || 0) + 1;
    const said = Object.entries(counts).map(([a, n]) => `${n} ${WORDS[a] || a}`).join(", ");
    rows.push(el("tr", {}, [el("th", { text: title }), el("td", { text: said })]));
  }
  out.push(rows.length ? el("table.project-counts", {}, rows) : el("p.dim", { text: "Nothing in this file changes this mixer." }));
  const lines = plan.changes.filter((c) => c.action !== "keep").map((c) => line(c));
  if (lines.length) out.push(el("details", {}, [el("summary", { text: `Every change (${lines.length})` }), el("ul.sm", {}, lines)]));
  if (plan.waiting.length) out.push(list("Still to do afterwards", plan.waiting));
  if (plan.needs_restart.length) out.push(list("Waits for a restart", plan.needs_restart));
  return out;
}

function line(c) {
  const what = `${c.part} ${c.id}`;
  const text = c.action === "rename" ? `${what} arrives as ${c.to}` : `${what}: ${WORDS[c.action] || c.action}`;
  return el("li", { text: c.note ? `${text} (${c.note})` : text });
}

function list(title, items) {
  return el("div.col", {}, [el("strong", { text: title }), el("ul.sm", {}, items.map((t) => el("li", { text: t })))]);
}

function finished(done) {
  const left = [...done.failed, ...done.waiting];
  if (!left.length && !done.needs_restart.length) return toast({ text: `Opened ${done.name || "the project"}.` });
  modal({
    title: `Opened ${done.name || "the project"}`,
    body: el("div.col", {}, [
      done.failed.length ? list("Did not start", done.failed) : null,
      done.waiting.length ? list("Still to do", done.waiting) : null,
      done.needs_restart.length ? list("Waits for a restart", done.needs_restart) : null,
    ]),
  });
}
