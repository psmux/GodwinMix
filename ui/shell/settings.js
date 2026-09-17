// Settings: the store, the global modal, and the drawer beside the tray.
//
// Two tabs, simple first. Nothing a volunteer does not need is on the first
// tab. The drawer is a different thing: it is the selected item's own form,
// rendered from its schema, in the `sidebar` slot, so a plugin can add a
// section to its own item's drawer and to nothing else.

import { el, clear, on } from "./dom.js";
import { modal } from "./modal.js";
import { allThemes, applyTheme, current as currentTheme } from "./theme.js";
import { DEFAULT_MAP, loadMap } from "./keymap.js";
import * as layout from "./layout.js";
import { list as panelList } from "./registry.js";
import { toast } from "./toast.js";

const KEY = "gmx.settings";

export const GALLERY_MODES = [
  ["live", "Live", "The picture, moving. Costs the multiview stream."],
  ["snapshot", "Snapshot", "A still, refreshed when you click it or on a timer."],
  ["icon", "Icon", "A coloured button with the kind's icon. Costs nothing."],
  ["label", "Label", "Colour and name only. Costs nothing."],
];

const DEFAULTS = {
  gallery: "live",
  snapshotSecs: 0,
  producer: false,
  meters: true,
  faders: true,
  lanes: true,
  tileWidth: 168,
  multiviewFps: 8,
  confirmRemove: true,
  confirmTake: true,
};

let state = null;
const listeners = new Set();

export function settings() {
  if (state) return state;
  let saved = {};
  try {
    saved = JSON.parse(localStorage.getItem(KEY) || "{}");
  } catch {
    saved = {};
  }
  state = Object.assign({}, DEFAULTS, saved);
  return state;
}

export function setSetting(key, value) {
  settings()[key] = value;
  try {
    localStorage.setItem(KEY, JSON.stringify(state));
  } catch {
    /* the choice lasts the session */
  }
  for (const fn of listeners) fn(state, key);
}

export function onSettingsChanged(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/**
 * The machine proposes the default once: a Pi class machine starts on icons, a
 * laptop on battery on snapshots, everything else live. `gmx doctor` will do
 * this properly from the core side; until then `hardwareConcurrency` and the
 * battery API are what a browser can see, and they are only used to choose a
 * first value the operator can change.
 */
export function proposeGalleryMode() {
  try {
    if (localStorage.getItem(KEY)) return null;
  } catch {
    /* fall through and propose */
  }
  const cores = navigator.hardwareConcurrency || 4;
  if (cores <= 4) return "icon";
  return "live";
}

// ---------------------------------------------------------------- the modal

export function openSettings(client, opts = {}) {
  const s = settings();
  const simple = el("div.form");
  const advanced = el("div.form");
  const body = el("div");
  const tabs = el("div.tabs");
  let shown = opts.tab === "advanced" ? advanced : simple;

  function tab(label, node) {
    const b = el("button", {
      text: label,
      onclick: () => {
        shown = node;
        for (const other of tabs.children) other.classList.toggle("on", other === b);
        clear(body);
        body.appendChild(node);
      },
    });
    if (node === shown) b.classList.add("on");
    tabs.appendChild(b);
    return b;
  }

  // --------------------------------------------------------- simple tab

  simple.appendChild(
    field("Theme", select(allThemes().map((t) => [t.id, t.title]), currentTheme(), (v) => applyTheme(v)))
  );

  simple.appendChild(
    field(
      "Tile pictures",
      select(GALLERY_MODES.map(([id, title]) => [id, title]), s.gallery, (v) => setSetting("gallery", v)),
      (GALLERY_MODES.find(([id]) => id === s.gallery) || GALLERY_MODES[0])[2]
    )
  );

  simple.appendChild(
    check("Producer mode: tap arms, Take puts it on air", s.producer, (v) => setSetting("producer", v))
  );

  simple.appendChild(check("Show meters on tiles", s.meters, (v) => setSetting("meters", v)));
  simple.appendChild(check("Show faders on tiles", s.faders, (v) => setSetting("faders", v)));
  simple.appendChild(check("Show the scrubber on files", s.lanes, (v) => setSetting("lanes", v)));
  simple.appendChild(check("Ask before removing anything", s.confirmRemove, (v) => setSetting("confirmRemove", v)));
  simple.appendChild(
    check("Ask before one source replaces a scene on air", s.confirmTake, (v) => setSetting("confirmTake", v))
  );

  simple.appendChild(
    field(
      "Setting up",
      el("button.btn", {
        text: "Show the welcome tiles again",
        onclick: async () => {
          const { showWelcomeAgain } = await import("../panels/welcome/panel.js");
          m.close();
          showWelcomeAgain();
        },
      }),
      "The five tiles this mixer opened with. Picking one applies its preset over what is here now."
    )
  );

  // ------------------------------------------------------- advanced tab

  advanced.appendChild(
    field(
      "Tile width",
      number(s.tileWidth, 96, 480, (v) => setSetting("tileWidth", v)),
      "Pixels. Pictures are asked for at this width times the screen's pixel ratio, never more."
    )
  );
  advanced.appendChild(
    field("Picture rate", number(s.multiviewFps, 1, 30, (v) => setSetting("multiviewFps", v)), "Frames a second for live tiles. Lower costs the mixer less.")
  );
  advanced.appendChild(
    field("Snapshot refresh", number(s.snapshotSecs, 0, 600, (v) => setSetting("snapshotSecs", v)), "Seconds between snapshot refreshes. 0 means only when you click a tile.")
  );

  advanced.appendChild(el("div.group-title", { text: "Connection" }));
  advanced.appendChild(
    field("Protocol", el("input", { type: "text", value: client.legacy ? "REST and /ws (this mixer has no /rpc yet)" : "JSON-RPC over /rpc", readonly: true }))
  );
  advanced.appendChild(
    el("button.btn", {
      text: "Forget the saved token",
      onclick: () => {
        try {
          localStorage.removeItem("gmx.token");
        } catch {
          /* nothing saved */
        }
        toast({ text: "Token forgotten. Reload the page to enter a new one." });
      },
    })
  );

  advanced.appendChild(el("div.group-title", { text: "Panels" }));
  const panels = el("div.col");
  for (const p of panelList()) {
    panels.appendChild(
      el("div.row", {}, [
        el("span.grow", { text: p.title }),
        el("span.dim.sm", { text: `${p.tier} · ${p.plugin}` }),
      ])
    );
  }
  advanced.appendChild(panels);

  advanced.appendChild(el("div.group-title", { text: "Layout" }));
  advanced.appendChild(
    el("button.btn", {
      text: "Reset the layout",
      onclick: () => {
        layout.reset();
        toast({ text: "Layout reset. Reload the page to see it." });
      },
    })
  );

  advanced.appendChild(el("div.group-title", { text: "Keyboard" }));
  const keys = el("div.col.sm");
  const map = loadMap();
  for (const [chord, id] of Object.entries(map)) {
    if (!DEFAULT_MAP[chord] && !id) continue;
    keys.appendChild(el("div.row", {}, [el("span.num.grow", { text: chord }), el("span.dim", { text: id })]));
  }
  advanced.appendChild(keys);

  tab("Simple", simple);
  tab("Advanced", advanced);
  clear(body);
  body.appendChild(shown);

  const m = modal({
    title: "Settings",
    body: el("div", {}, [tabs, body]),
    footer: [el("button.btn.primary", { text: "Done", onclick: () => m.close() })],
  });
  return m;
}

// ---------------------------------------------------------------- controls

function field(label, control, hint) {
  return el("div.field", {}, [
    el("label", {}, [el("span.lbl", { text: label }), control]),
    hint ? el("span.hint", { text: hint }) : null,
  ]);
}

function check(label, value, onChange) {
  const input = el("input", { type: "checkbox", checked: !!value, style: { width: "auto" } });
  on(input, "change", () => onChange(input.checked));
  return el("div.field", {}, [el("label.inline", {}, [input, el("span", { text: label })])]);
}

function select(options, value, onChange) {
  const node = el("select");
  for (const [id, title] of options) node.appendChild(el("option", { value: id, text: title }));
  node.value = value;
  on(node, "change", () => onChange(node.value));
  return node;
}

function number(value, min, max, onChange) {
  const input = el("input", { type: "number", value: String(value), min: String(min), max: String(max) });
  on(input, "change", () => {
    const n = Number(input.value);
    if (Number.isFinite(n)) onChange(Math.min(max, Math.max(min, n)));
  });
  return input;
}
