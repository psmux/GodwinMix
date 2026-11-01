// The controls of a look on one item: a grid of pictures, each the camera as
// it would be with that look, so a look is picked by seeing it, not by name.
//
// The pictures come from `lut.preview`, which grades a still of the item's
// own camera through the same table the frame goes through (a test card of
// skin, sky and greys when there is no camera). Every change is
// `scene.item.filter.set`, applied to the running filter in place, so the
// canvas beside this shows the look on air as it is picked.

import { el, on, clear } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { withShow } from "../../client/transport-rpc.js";
import { withParam } from "./key.js";
import { SLIDERS as COLOUR } from "./colour.js";

/** The tabs: which LUT group each shows, and what it is called. */
export const TABS = [
  ["look", "Looks"],
  ["camera", "Camera log"],
  ["imported", "Imported"],
  ["preset", "Presets"],
];

export const ACCEPT = ".cube,.3dl,.png";

/** Which tab a filter's params belong on when the editor opens. */
export function tabFor(params, luts) {
  if (params.preset) return "preset";
  const hit = luts.find((l) => l.name === params.lut);
  return hit ? hit.group : "look";
}

/** One file to `POST /api/v1/luts/upload`, which imports it. */
export function upload(client, file) {
  return new Promise((resolve, reject) => {
    const t = client.transport || {};
    const u = withShow(new URL("/api/v1/luts/upload", t.base || location.href));
    u.searchParams.set("name", file.name);
    const xhr = new XMLHttpRequest();
    xhr.open("POST", u.toString());
    xhr.setRequestHeader("Content-Type", "application/octet-stream");
    if (t.token) xhr.setRequestHeader("Authorization", "Bearer " + t.token);
    xhr.onload = () => (xhr.status < 300 ? resolve(JSON.parse(xhr.responseText || "{}")) : reject(new Error(messageOf(xhr.responseText))));
    xhr.onerror = () => reject(new Error("the file could not reach the mixer"));
    xhr.send(file);
  });
}

function messageOf(text) {
  try {
    const body = JSON.parse(text);
    return (body.error && body.error.message) || body.message || text;
  } catch {
    return text;
  }
}

/**
 * @param {{client: object, filter: object, ref: string|number, source?: string, set: (ref, params) => Promise}} o
 */
export function lutEditor(o) {
  let params = Object.assign({}, o.filter.params || {});
  let library = { luts: [], presets: [] };
  let pictures = new Map();
  let tab = "look";
  let timer = null;
  const send = (now) => {
    clearTimeout(timer);
    const go = () => o.set(o.ref, params).catch((e) => errorToast(e, "Look"));
    if (now) go();
    else timer = setTimeout(go, 120);
  };

  const tabs = el("div.lut-tabs", { role: "tablist" });
  const grid = el("div.lut-grid", { role: "listbox", "aria-label": "Looks" });
  const status = el("span.sm.faint.grow");

  const amount = slider("Amount", typeof params.amount === "number" ? params.amount : 1, 0, 1, 0.01, (v) => {
    params = withParam(params, "amount", v);
    send(false);
  }, (v) => `${Math.round(v * 100)}%`);

  const adjust = el("details.lut-adjust", {}, [
    el("summary.sm", { text: "Adjust brightness, contrast and colour" }),
    ...COLOUR.map(([key, text, min, max, step]) =>
      slider(text, typeof params[key] === "number" ? params[key] : 0, min, max, step, (v) => {
        params = withParam(params, key, v);
        send(false);
      })
    ),
  ]);

  function card(name, title, kind, about) {
    const chosen = kind === "preset" ? params.preset === name : !params.preset && (params.lut || "") === name;
    const img = pictures.get(`${kind}:${name}`);
    const pic = img ? el("img", { src: img, alt: "", loading: "lazy", draggable: "false" }) : el("div.lut-pic-wait");
    const b = el("button.lut-card" + (chosen ? ".on" : ""), {
      type: "button",
      role: "option",
      "aria-selected": chosen ? "true" : "false",
      title: about || title,
      onclick: () => pick(name, kind),
    }, [pic, el("span.lut-name", { text: title })]);
    return b;
  }

  function pick(name, kind) {
    if (kind === "preset") {
      params = Object.assign({}, { preset: name });
    } else {
      params = withParam(params, "lut", name);
      delete params.preset;
      if (!name) delete params.lut;
    }
    send(true);
    draw();
  }

  function draw() {
    clear(tabs);
    for (const [id, text] of TABS) {
      const count = id === "preset" ? library.presets.length : library.luts.filter((l) => l.group === id).length;
      tabs.appendChild(el("button.chip" + (tab === id ? ".on" : ""), {
        type: "button",
        role: "tab",
        "aria-selected": tab === id ? "true" : "false",
        text: count ? `${text} ${count}` : text,
        onclick: () => {
          tab = id;
          draw();
        },
      }));
    }
    clear(grid);
    grid.appendChild(card("", "None", "lut", "No look: the camera as it is"));
    if (tab === "preset") {
      for (const p of library.presets) grid.appendChild(card(p.name, p.title || p.name, "preset", presetAbout(p)));
      if (!library.presets.length) grid.appendChild(el("p.sm.dim.lut-empty", { text: "No presets yet. Pick a look, set the amount and colour, then Save as preset." }));
    } else {
      for (const l of library.luts.filter((x) => x.group === tab)) grid.appendChild(card(l.name, l.title, "lut", l.about));
      if (tab === "imported" && !library.luts.some((l) => l.group === "imported")) {
        grid.appendChild(el("p.sm.dim.lut-empty", { text: "Drop a .cube, .3dl or .png LUT here, or press Import." }));
      }
    }
  }

  async function load() {
    status.textContent = "Loading looks";
    try {
      library = await o.client.call("lut.list", {});
      tab = tabFor(params, library.luts);
      draw();
      const shot = await o.client.call("lut.preview", { source: o.source, width: 200, presets: library.presets.map((p) => p.name) });
      pictures = new Map([["lut:", shot.original], ...shot.previews.map((p) => [`${p.kind}:${p.name}`, p.image])]);
      status.textContent = shot.source ? `Shown on ${shot.source}` : "Shown on a test card";
      draw();
    } catch (e) {
      status.textContent = "";
      errorToast(e, "Looks");
    }
  }

  async function importFiles(files) {
    for (const file of files) {
      status.textContent = `Importing ${file.name}`;
      try {
        const done = await upload(o.client, file);
        tab = "imported";
        pick(done.lut.name, "lut");
      } catch (e) {
        errorToast(e, "Import LUT");
      }
    }
    await load();
  }

  const picker = el("input", { type: "file", accept: ACCEPT, multiple: true, hidden: true });
  on(picker, "change", () => importFiles([...picker.files]));
  // Naming a preset happens in place: a field and a button under the grid.
  const nameField = el("input", { type: "text", placeholder: "Name, such as Church warm", "aria-label": "Preset name" });
  const naming = el("div.row.lut-naming", { hidden: true }, [nameField, el("button.btn.sm.primary", { text: "Save", onclick: () => savePreset() })]);
  on(nameField, "keydown", (e) => {
    if (e.key === "Enter") savePreset();
    if (e.key === "Escape") naming.hidden = true;
  });
  const save = el("button.btn.sm", {
    text: "Save as preset",
    onclick: () => {
      naming.hidden = !naming.hidden;
      if (!naming.hidden) nameField.focus();
    },
  });
  async function savePreset() {
    const title = nameField.value.trim();
    if (!title) return nameField.focus();
    const keep = Object.fromEntries(["lut", "amount", ...COLOUR.map((c) => c[0])].filter((k) => params[k] !== undefined).map((k) => [k, params[k]]));
    try {
      const saved = await o.client.call("lut.preset.save", Object.assign({ name: title, title }, keep));
      naming.hidden = true;
      nameField.value = "";
      tab = "preset";
      pick(saved.name, "preset");
      await load();
    } catch (e) {
      errorToast(e, "Save preset");
    }
  }

  const root = el("div.col.key-editor.lut-editor", {}, [
    tabs,
    grid,
    amount,
    adjust,
    el("div.row", {}, [save, el("button.btn.sm", { text: "Import LUT", title: "A .cube, .3dl or .png LUT", onclick: () => picker.click() }), picker]),
    naming,
    el("div.row", {}, [status]),
  ]);
  on(root, "dragover", (e) => {
    e.preventDefault();
    root.classList.add("drop-into");
  });
  on(root, "dragleave", () => root.classList.remove("drop-into"));
  on(root, "drop", (e) => {
    e.preventDefault();
    root.classList.remove("drop-into");
    importFiles([...(e.dataTransfer?.files || [])]);
  });
  load();
  return root;
}

function presetAbout(p) {
  const parts = [];
  if (p.lut) parts.push(p.lut);
  if (typeof p.amount === "number") parts.push(`${Math.round(p.amount * 100)}%`);
  return parts.join(", ") || p.title || p.name;
}

function slider(text, value, min, max, step, change, show) {
  const fmt = show || ((v) => String(v));
  const input = el("input", { type: "range", min: String(min), max: String(max), step: String(step), value: String(value), "aria-label": text });
  const shown = el("span.sm.faint", { text: fmt(value) });
  on(input, "input", () => {
    shown.textContent = fmt(Number(input.value));
    change(Number(input.value));
  });
  return el("label.field", {}, [el("span.lbl", { text }), input, shown]);
}
