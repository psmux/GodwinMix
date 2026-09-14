// The add input and add output picker.
//
// OBS shows a text list of source types. This shows a tile per kind with an
// icon, one line of description and the plugin that provides it, grouped, with
// a search box and a compact list view for a narrow window. Picking a tile
// opens that kind's settings form, generated from its schema.
//
// It is static: the icons are inline SVG and the catalogue is fetched once, so
// opening it costs one call on a cold start and nothing after that.

import { el, clear, svg, on } from "./dom.js";
import { modal } from "./modal.js";
import { toast, errorToast } from "./toast.js";
import { ICONS, SOURCE_KINDS, OUTPUT_KINDS, loadKinds, grouped, kindOfUri } from "../client/kinds.js";
import { SchemaForm } from "../client/schema-form.js";

const LIST_KEY = "gmx.picker.list";

/**
 * @param {object} client
 * @param {"source"|"output"} what
 * @param {{preset?: object, kind?: string}} opts  a dropped file or URL arrives
 *        here as `{preset: {uri}, kind}`, which skips straight to the form.
 */
export async function openPicker(client, what, opts = {}) {
  const kinds = await loadKinds(client, what).catch(() => (what === "output" ? OUTPUT_KINDS : SOURCE_KINDS));
  if (opts.kind) {
    const chosen = kinds.find((k) => k.id === opts.kind);
    if (chosen) return openForm(client, what, chosen, opts.preset);
  }

  let listView = false;
  try {
    listView = localStorage.getItem(LIST_KEY) === "1";
  } catch {
    /* no storage, no preference */
  }

  const search = el("input", { type: "text", placeholder: "Search", "aria-label": "Search kinds" });
  const grid = el("div.picker-grid");
  const body = el("div", {}, [search, grid]);

  const toggle = el("button.btn", {
    text: listView ? "Tiles" : "List",
    onclick: () => {
      listView = !listView;
      toggle.textContent = listView ? "Tiles" : "List";
      try {
        localStorage.setItem(LIST_KEY, listView ? "1" : "0");
      } catch {
        /* nothing to remember it with */
      }
      draw();
    },
  });

  const m = modal({
    title: what === "output" ? "Add an output" : "Add an input",
    body,
    footer: [toggle, el("button.btn", { text: "Cancel", onclick: () => m.close() })],
    wide: true,
  });

  function draw() {
    const q = search.value.trim().toLowerCase();
    const matching = kinds.filter(
      (k) =>
        !q ||
        k.title.toLowerCase().includes(q) ||
        k.description.toLowerCase().includes(q) ||
        k.id.toLowerCase().includes(q) ||
        String(k.plugin).toLowerCase().includes(q)
    );
    clear(grid);
    grid.className = "picker-grid" + (listView ? " list" : "");
    if (!matching.length) {
      grid.appendChild(el("p.dim", { text: `Nothing matches "${search.value}".` }));
      return;
    }
    for (const [group, items] of grouped(matching)) {
      const section = el("div");
      section.appendChild(el("div.group-title", { text: group }));
      const inner = el("div.picker-grid" + (listView ? ".list" : ""));
      for (const kind of items) inner.appendChild(tile(kind));
      section.appendChild(inner);
      grid.appendChild(section);
    }
  }

  function tile(kind) {
    return el(
      "button.kindtile",
      {
        onclick: () => {
          m.close();
          openForm(client, what, kind, opts.preset);
        },
      },
      [
        svg(ICONS[kind.icon] || ICONS.stream, 26),
        el("span.grow", {}, [
          el("div", { text: kind.title }),
          el("div.dim.sm", { text: kind.description }),
          el("div.who", { text: kind.plugin }),
        ]),
      ]
    );
  }

  on(search, "input", draw);
  draw();
  search.focus();
  return m;
}

/** The chosen kind's form, rendered from its schema and nothing else. */
export function openForm(client, what, kind, preset) {
  const form = new SchemaForm(kind.schema, preset || {});
  const add = el("button.btn.primary", { text: what === "output" ? "Start sending" : "Add" });

  const m = modal({
    title: kind.title,
    body: el("div", {}, [el("p.dim.sm", { text: kind.description, style: { marginTop: "0" } }), form.el]),
    footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() }), add],
  });

  on(form.el, "keydown", (e) => {
    if (e.key === "Enter" && e.target.tagName === "INPUT") {
      e.preventDefault();
      add.click();
    }
  });

  add.onclick = async () => {
    if (!form.validate()) {
      toast({ kind: "warning", text: "Fill in the fields outlined in red." });
      return;
    }
    add.disabled = true;
    try {
      const params = kind.build(form.read());
      await client.call(what === "output" ? "output.add" : "source.add", params);
      m.close();
      toast({ text: what === "output" ? "Sending started." : "Added. It appears in the tray as soon as it connects." });
    } catch (e) {
      errorToast(e, kind.title);
      add.disabled = false;
    }
  };
  form.focusFirst();
  return m;
}

/**
 * Something was dropped on the window. A file, a URL or an RTMP address skips
 * the picker: the scheme picks the kind, exactly as the URL detection in the
 * old page did, and the form opens with the address already filled in.
 */
export function pickFromDrop(client, text) {
  const uri = String(text || "").trim();
  if (!uri) return;
  const id = kindOfUri(uri);
  const kind = SOURCE_KINDS.find((k) => k.id === id) || SOURCE_KINDS[0];
  openForm(client, "source", kind, { uri });
}
