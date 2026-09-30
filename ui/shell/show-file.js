// File > New show and File > Switch show, which the + on the show tabs and
// the palette use as well. New show makes an empty show, a copy of the one
// this page is on, or one from a saved project file, with `show.add`.

import { el } from "./dom.js";
import { modal } from "./modal.js";
import { contextMenu } from "./menu.js";
import { toast, errorToast } from "./toast.js";
import { switchTo, sheet } from "./show-actions.js";

const here = (answer) => new URLSearchParams(location.search).get("show") || answer.current;

async function list(client, what) {
  try {
    return await client.call("show.list", {});
  } catch (e) {
    errorToast(e, what);
    return null;
  }
}

/** One of three ways to start, as radio cards. */
function choice(name, value, title, line, checked) {
  return el("label.show-from", {}, [
    el("input", { type: "radio", name, value, checked: !!checked }),
    el("span", {}, [el("strong", { text: title }), el("span.dim.sm", { text: line })]),
  ]);
}

export async function newShow(client) {
  sheet();
  const answer = await list(client, "New show");
  if (!answer) return;
  const shows = answer.shows || [];
  const current = shows.find((s) => s.id === here(answer));
  const group = `from-${Date.now()}`;
  const name = el("input", { type: "text", value: `Show ${shows.length + 1}`, "aria-label": "Name", autocomplete: "off" });
  const picked = el("span.dim.sm", { text: "No file chosen" });
  let project = null;
  const file = el("input", { type: "file", accept: ".gmxproject,.json,application/json", hidden: true });
  const from = el("div.show-froms", { role: "radiogroup", "aria-label": "Start from" }, [
    choice(group, "empty", "Empty", "No sources, scenes or outputs yet.", true),
    current ? choice(group, current.id, `A copy of ${current.name}`, "Its scenes and sources, without its outputs, so nothing goes out twice.") : null,
    choice(group, "project", "From a project file", "A file saved with File, Save project as."),
  ]);
  const choose = el("button.btn", { type: "button", text: "Choose file…", onclick: () => file.click() });
  const fileRow = el("div.row", { hidden: true }, [choose, picked, file]);
  from.addEventListener("change", () => {
    fileRow.hidden = value() !== "project";
    if (!fileRow.hidden && !project) file.click();
  });
  file.addEventListener("change", async () => {
    const f = file.files && file.files[0];
    if (!f) return;
    try {
      project = JSON.parse(await f.text());
      picked.textContent = f.name;
    } catch {
      project = null;
      picked.textContent = `${f.name} is not a project file. Choose one saved with Save project as.`;
    }
  });
  const value = () => from.querySelector("input:checked").value;
  const make = el("button.btn.primary", { text: "Make show" });
  const body = el("div.show-new", {}, [el("label.field", {}, [el("span", { text: "Name" }), name]), el("span.dim.sm", { text: "Start from" }), from, fileRow]);
  const m = modal({ title: "New show", body, footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() }), make] });
  name.select();
  body.addEventListener("keydown", (e) => e.key === "Enter" && e.target === name && make.click());
  make.onclick = async () => {
    const v = value();
    if (!name.value.trim()) return toast({ kind: "warning", text: "Give the show a name." }), name.focus();
    if (v === "project" && !project) return toast({ kind: "warning", text: "Choose a project file first, or start from empty." });
    make.disabled = true;
    try {
      const show = await client.call("show.add", { name: name.value.trim(), from: v === "project" ? { project } : v });
      m.close();
      toast({ text: `${show.name || name.value.trim()} is ready. Switching to it.` });
      setTimeout(() => switchTo(show.id), 600);
    } catch (e) {
      make.disabled = false;
      errorToast(e, "New show");
    }
  };
}

/** A menu of every show, or straight to one by id. */
export async function switchShow(client, id) {
  if (id) return switchTo(id);
  const answer = await list(client, "Switch show");
  if (!answer) return;
  const current = here(answer);
  const row = document.querySelector(".shows")?.getBoundingClientRect();
  contextMenu(row ? row.left : 80, row ? row.bottom + 4 : 60, (answer.shows || []).map((s) => ({
    label: `${s.id === current ? "✓ " : ""}${s.name}${s.on_air ? ", on air" : s.state !== "running" ? `, ${s.state}` : ""}`,
    run: () => s.id !== current && switchTo(s.id),
  })));
}
