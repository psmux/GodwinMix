// File > Save project as, Open project and New project.
//
// The file is what `project.export` answers, saved as it is: one JSON
// document. The page adds its own part (the workspace, its settings and its
// theme) so that opening the file on another machine looks the same too.

import { el } from "./dom.js";
import { modal, confirmModal } from "./modal.js";
import { settings, setSetting } from "./settings.js";
import { applyTheme, current as currentTheme } from "./theme.js";
import { toast, errorToast } from "./toast.js";

export const EXTENSION = ".gmxproject";
const DESKTOP = /GodwinMix-Desktop/.test(navigator.userAgent);
const NAME_KEY = "gmx.project.name";

function lastName() {
  try { return localStorage.getItem(NAME_KEY) || "GodwinMix project"; } catch { return "GodwinMix project"; }
}

/** What this page puts in the file beside the mixer's parts. */
export function pageState() {
  const w = document.querySelector("gmx-shell")?.workspace;
  let keys = null;
  try { keys = JSON.parse(localStorage.getItem("gmx.keys") || "null"); } catch { /* none saved */ }
  return { version: 1, settings: { ...settings() }, theme: currentTheme(), workspace: w ? { version: 1, ...w.state } : null, keys };
}

/** Put a file's page part back: the workspace, the settings and the theme. */
export async function applyPage(page) {
  if (!page || page.version !== 1) return;
  for (const [key, value] of Object.entries(page.settings || {})) setSetting(key, value);
  if (page.theme) applyTheme(page.theme);
  if (page.keys) try { localStorage.setItem("gmx.keys", JSON.stringify(page.keys)); } catch { /* keys last the session */ }
  const w = document.querySelector("gmx-shell")?.workspace;
  if (w && page.workspace) {
    const { decodeLayout } = await import("./dock-presets.js");
    try {
      w.state = decodeLayout(page.workspace);
      w.sync();
    } catch (e) {
      toast({ text: "The project's panel layout was not used: " + e.message });
    }
  }
}

export function saveProject(client) {
  const name = el("input", { type: "text", value: lastName(), maxlength: 80, "aria-label": "Project name" });
  const secrets = el("input", { type: "checkbox" });
  const media = el("input", { type: "checkbox" });
  const status = el("p.sm.dim", { role: "status" });
  const save = el("button.btn.primary", { text: "Save" });
  const dialog = modal({
    title: "Save project as",
    body: el("div.col", {}, [
      el("label.col", {}, [el("span", { text: "Name" }), name]),
      el("label.row", {}, [secrets, el("span", { text: "Include stream keys and channel keys" })]),
      el("p.sm.dim", { text: "Leave this off for a file you will send to someone. With it off, destinations and channels come back without their keys and ask for them again." }),
      el("label.row", {}, [media, el("span", { text: "Include the clips themselves" })]),
      el("p.sm.dim", { text: "Off, the file lists clips by name and size and they are copied separately. On, a big media folder makes a big file." }),
      status,
    ]),
    footer: [el("button.btn", { text: "Cancel", onclick: () => dialog.close() }), save],
  });
  save.onclick = async () => {
    const title = name.value.trim() || "GodwinMix project";
    try { localStorage.setItem(NAME_KEY, title); } catch { /* remembered for the session only */ }
    const ask = { name: title, include_secrets: secrets.checked, include_media: media.checked, page: pageState() };
    if (DESKTOP) {
      dialog.close();
      return desktopSave(ask);
    }
    save.disabled = true;
    status.textContent = "Saving…";
    try {
      const file = await client.call("project.export", ask);
      download(file, fileName(title));
      dialog.close();
      toast({ text: `Saved ${fileName(title)}${file.removed && file.removed.length ? ", without its keys" : ""}.` });
    } catch (e) {
      status.textContent = "";
      save.disabled = false;
      errorToast(e, "Save project");
    }
  };
}

export function fileName(title) {
  return (title.replace(/[^\w\- ]+/g, "").trim().replace(/\s+/g, "-") || "project") + EXTENSION;
}

function download(file, name) {
  const url = URL.createObjectURL(new Blob([JSON.stringify(file, null, 2)], { type: "application/json" }));
  const link = el("a", { href: url, download: name });
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 5000);
}

/** In the desktop app the shell asks where, with the system's own dialog. */
function desktopSave(ask) {
  const page = btoa(unescape(encodeURIComponent(JSON.stringify(ask.page))));
  const q = new URLSearchParams({ name: ask.name, secrets: ask.include_secrets ? "1" : "0", media: ask.include_media ? "1" : "0" });
  location.href = `godwinmix://save-project?${q}#${page}`;
}

export function openProject(client, file) {
  if (file) return readThen(client, file);
  const input = el("input", { type: "file", accept: `${EXTENSION},.json,application/json`, hidden: true });
  input.addEventListener("change", () => {
    const chosen = input.files && input.files[0];
    input.remove();
    if (chosen) readThen(client, chosen);
  });
  document.body.append(input);
  input.click();
}

async function readThen(client, file) {
  let parsed;
  try {
    parsed = JSON.parse(await file.text());
  } catch {
    return toast({ text: `${file.name} is not a GodwinMix project: it is not JSON. Choose a ${EXTENSION} file saved by Save project as.` });
  }
  const { review } = await import("./project-open.js");
  return review(client, parsed, file.name);
}

export async function newProject(client) {
  const yes = await confirmModal(
    "A new project takes every source, output, channel and scene off this mixer, including anything on air. Save this one first if you want it back.",
    "Start a new project"
  );
  if (!yes) return;
  const empty = { format: "godwinmix.project", version: 1, name: "a new, empty project" };
  try {
    const done = await client.call("project.import", { file: empty, mode: "replace", dry_run: false });
    // Started again, as after Open project, so every panel draws the empty mixer.
    sessionStorage.setItem("gmx.project.opened", JSON.stringify(done));
    location.reload();
  } catch (e) {
    errorToast(e, "New project");
  }
}
