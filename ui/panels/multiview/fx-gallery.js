// Transitions and effects from packs, beside Take.
//
// A "Looks" button opens a panel of everything in the fx library, each with
// a moving preview: a strip of twelve frames the core made once, moved by a
// CSS step animation, so the page decodes nothing and the moving stops when
// the panel closes (a hidden element is not painted). Tap one to use it for
// the next take, or keep it as the default or the armed scene's own. An
// Import button and a drop anywhere on the panel send a clip, a picture, a
// shader or a whole zip to the core, which works out what it is.
//
// Effects, the items that play over the programme without changing it, get
// a row of buttons of their own under the take bar, and Alt+1 to Alt+9 (the
// keyboard map has them; a Mac whose Option key types a symbol can rebind
// them in the shortcuts list).

import { el, on } from "../../shell/dom.js";
import { toast, errorToast } from "../../shell/toast.js";
import { register } from "../../shell/commands.js";
import { withShow } from "../../client/transport-rpc.js";

const ACCEPT = ".zip,.webm,.mov,.mp4,.m4v,.mkv,.gif,.png,.jpg,.jpeg,.tif,.tiff,.webp,.glsl,.frag";

/** A URL on the core, with the token, for an `<img>` or a CSS background. */
function coreUrl(client, path) {
  const t = client.transport || {};
  const u = new URL(path, t.base || location.href);
  if (t.token) u.searchParams.set("token", t.token);
  return withShow(u).toString();
}

/** One file to `POST /api/v1/fx/upload`, which imports it. */
function upload(client, file) {
  return new Promise((resolve, reject) => {
    const t = client.transport || {};
    const u = withShow(new URL("/api/v1/fx/upload", t.base || location.href));
    u.searchParams.set("name", file.name);
    const xhr = new XMLHttpRequest();
    xhr.open("POST", u.toString());
    xhr.setRequestHeader("Content-Type", "application/octet-stream");
    if (t.token) xhr.setRequestHeader("Authorization", "Bearer " + t.token);
    xhr.onload = () => (xhr.status < 300 ? resolve(JSON.parse(xhr.responseText || "{}")) : reject(new Error(xhr.responseText)));
    xhr.onerror = () => reject(new Error("the file could not reach the mixer"));
    xhr.send(file);
  });
}

/** A tile: the moving strip, the name, and what it is. */
export function tile(client, item, actions) {
  const strip = el("div.fx-strip", { role: "img", "aria-label": `${item.title || item.name} preview` });
  strip.style.setProperty("--frames", "12");
  strip.style.setProperty("--ms", `${Math.max(800, Math.min(item.duration_ms || 1000, 4000))}ms`);
  strip.dataset.src = coreUrl(client, item.preview);
  const what = [item.kind, item.blend !== "normal" ? item.blend : "", item.runs || ""].filter(Boolean).join(" · ");
  return el("div.fx-tile", { title: item.note || what }, [strip, el("strong.ellipsis", { text: item.title || item.name }), el("small", { text: what }), el("div.fx-actions", {}, actions)]);
}

/** The Looks button and its panel. `pick(name)` chooses a transition. */
export function gallery(client, { pick, armedScene }) {
  const grid = el("div.fx-grid");
  const file = el("input", { type: "file", accept: ACCEPT, multiple: true, hidden: true });
  const status = el("div.fx-status", { role: "status" });
  const panel = el("div.fx-panel", { hidden: true }, [
    el("div.fx-head", {}, [el("strong", { text: "Transitions and effects" }), el("button.btn", { text: "Import", onclick: () => file.click() })]),
    status, grid, file,
  ]);
  const button = el("button.btn.fx-open", { text: "Looks", "aria-expanded": "false", title: "Imported transitions and effects", onclick: () => toggle() });

  async function assign(scene, name) {
    try {
      await client.call("fx.assign", scene ? { scene, transition: name } : { transition: name });
      toast({ text: scene ? `${name} is now ${scene}'s transition` : `${name} is now the default transition` });
      refresh();
    } catch (e) {
      errorToast(e, "Assign");
    }
  }

  async function refresh() {
    status.textContent = "Loading…";
    try {
      const list = await client.call("fx.list", {});
      const a = list.assigned || {};
      status.textContent = `${list.fx.length} items${list.gpu ? ", shaders on the GPU" : ""}${a.default ? `. Default: ${a.default}` : ""}`;
      grid.replaceChildren(...list.fx.filter((i) => i.transition).map((i) => {
        const scene = armedScene();
        const actions = [
          el("button.btn", { text: "Use", onclick: () => { pick(i.name); toast({ text: `Next take: ${i.title || i.name}` }); } }),
          el("button.btn", { text: a.default === i.name ? "Default ✓" : "Default", onclick: () => assign(null, i.name) }),
        ];
        if (scene) actions.push(el("button.btn", { text: (a.scenes || {})[scene] === i.name ? `${scene} ✓` : `For ${scene}`, onclick: () => assign(scene, i.name) }));
        return tile(client, i, actions);
      }));
      for (const s of grid.querySelectorAll(".fx-strip")) s.style.backgroundImage = `url("${s.dataset.src}")`;
    } catch (e) {
      status.textContent = "This mixer has no fx library.";
    }
  }

  async function send(files) {
    for (const f of files) {
      status.textContent = `Importing ${f.name}…`;
      try {
        const done = await upload(client, f);
        const n = (done.imported || []).length;
        toast({ text: done.task_id ? `${f.name} is importing in the background` : `${f.name}: ${n} imported${(done.skipped || []).length ? `, ${done.skipped.length} skipped` : ""}` });
      } catch (e) {
        errorToast(e, "Import");
      }
    }
    refresh();
  }

  function toggle(open = panel.hidden) {
    panel.hidden = !open;
    button.setAttribute("aria-expanded", String(open));
    if (open) refresh();
    else grid.replaceChildren();
  }

  on(file, "change", () => send([...file.files]));
  on(panel, "dragover", (e) => e.preventDefault());
  on(panel, "drop", (e) => {
    e.preventDefault();
    send([...(e.dataTransfer?.files || [])]);
  });
  return { button, panel, refresh, toggle };
}

/** The effect buttons, and their keys. Answers the element and a removal. */
export function effects(client) {
  const row = el("div.fx-effects", { role: "group", "aria-label": "Effects" });
  let offs = [];
  async function load() {
    offs.forEach((off) => off());
    offs = [];
    try {
      const list = await client.call("fx.list", { role: "effect" });
      row.replaceChildren(...list.fx.map((i, n) => {
        const fire = () => client.call("fx.fire", { name: i.name }).catch((e) => errorToast(e, "Effect"));
        if (n < 9) offs.push(register({ id: `fx.fire-${n + 1}`, title: `Play ${i.title || i.name} over the programme`, group: "Effects", key: `Alt+${n + 1}`, run: fire }));
        return el("button.btn.fx-fire", { text: i.title || i.name, title: `Play over the programme${n < 9 ? ` (Alt+${n + 1})` : ""}`, onclick: fire });
      }));
    } catch {
      row.hidden = true;
    }
  }
  load();
  return { el: row, reload: load, off: () => offs.forEach((off) => off()) };
}
