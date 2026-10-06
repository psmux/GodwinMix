// The transition picker: every transition this mixer takes, each moving, and
// the direction or colour, easing and length of the chosen one.
//
// Filled when it opens and emptied when it closes, so its moving previews
// cost nothing while it is shut: the built in ones are SVG drawings walked by
// CSS, and the fx library's are strips of twelve frames the core made once.
// Packs are imported here too, by the Import button or by a drop.

import { el, on } from "../../shell/dom.js";
import { toast, errorToast } from "../../shell/toast.js";
import { TYPES, EASINGS, DURATIONS } from "./transition-state.js";
import { iconSvg } from "./transition-icons.js";
import { tile, upload, ACCEPT } from "./fx-gallery.js";

function choice(state, type, label, body) {
  return el("button.tp-tile", {
    type: "button", "data-type": type, title: label,
    "aria-pressed": String(state.type === type),
    onclick: () => state.set({ type }),
  }, [el("span.tp-art", { html: body }), el("span.ellipsis", { text: label })]);
}

function select(label, pairs, value, onPick) {
  const s = el("select", { "aria-label": label, title: label },
    pairs.map(([v, t]) => el("option", { value: String(v), text: t, selected: String(v) === String(value) })));
  on(s, "change", () => onPick(s.value));
  return s;
}

/** The options row: the chosen one's direction or colour, easing and length. */
function options(state) {
  const k = state.kind();
  const pairs = state.options();
  const row = el("div.tp-options");
  if (pairs.length) row.append(select(k.option === "colour" ? "Colour" : "Direction", pairs, state.option, (v) => state.set({ option: v })));
  if (k.origin !== "plugin") row.append(select("Easing", EASINGS, state.easing, (v) => state.set({ easing: v })));
  const lengths = el("div.tp-lengths", { role: "group", "aria-label": "Length" }, DURATIONS.map(([ms, text]) =>
    el("button.btn", { type: "button", text, "aria-pressed": String(state.ms() === ms), disabled: Boolean(k.ownMs), onclick: () => state.set({ ms }) })));
  row.append(lengths);
  if (k.ownMs) row.append(el("small.dim", { text: `Runs for its own ${state.lengthText()}.` }));
  return row;
}

/** The sheet's element, and the fill and empty its popover calls. */
export function transitionSheet(state, { client, armedScene, done }) {
  const body = el("div.tp-body");
  const status = el("div.fx-status", { role: "status" });
  const file = el("input", { type: "file", accept: ACCEPT, multiple: true, hidden: true });
  const head = el("div.fx-head", {}, [
    el("strong", { text: "Transition for Take" }),
    el("span.grow"),
    client ? el("button.btn", { type: "button", text: "Import", title: "Add transitions from a pack: clips, pictures, shaders or a zip", onclick: () => file.click() }) : null,
    el("button.btn.primary", { type: "button", text: "Done", onclick: () => done() }),
  ]);
  const sheet = el("div.tp-sheet", { role: "dialog", "aria-label": "Transitions", "data-width": "620" }, [head, body, file]);
  let fx = [];
  let assigned = {};
  let filled = false;

  function paint() {
    if (!filled) return;
    const built = TYPES.map((t) => choice(state, t.type, t.label, iconSvg(t.type, true)));
    const added = [...state.extra.values()].filter((k) => k.origin !== "fx")
      .map((k) => choice(state, k.type, k.label, iconSvg(k.type)));
    body.replaceChildren(
      options(state),
      el("div.tp-grid", {}, [...built, ...added]),
      fx.length ? el("strong.tp-section", { text: "From packs" }) : null,
      status,
      fx.length ? el("div.fx-grid", {}, fx.map(packTile)) : null,
    );
  }

  function packTile(i) {
    const scene = armedScene();
    const use = () => {
      if (!state.extra.has(i.name)) state.learn([{ name: i.name, title: i.title, origin: "fx", type: i.kind, duration_ms: i.duration_ms }]);
      state.set({ type: i.name });
    };
    const actions = [
      el("button.btn", { type: "button", text: state.type === i.name ? "In use" : "Use", "aria-pressed": String(state.type === i.name), onclick: use }),
      el("button.btn", { type: "button", text: assigned.default === i.name ? "Default ✓" : "Default", title: "Use it whenever a take names no transition", onclick: () => assign(null, i.name) }),
    ];
    if (scene) actions.push(el("button.btn", { type: "button", text: (assigned.scenes || {})[scene] === i.name ? `${scene} ✓` : `For ${scene}`, onclick: () => assign(scene, i.name) }));
    return tile(client, i, actions);
  }

  async function assign(scene, name) {
    try {
      await client.call("fx.assign", scene ? { scene, transition: name } : { transition: name });
      toast({ text: scene ? `${name} is now ${scene}'s transition` : `${name} is now the default transition` });
      await refresh();
    } catch (e) {
      errorToast(e, "Assign");
    }
  }

  async function refresh() {
    if (!client) return;
    status.textContent = "Loading the fx library…";
    try {
      const list = await client.call("fx.list", {});
      fx = (list.fx || []).filter((i) => i.transition);
      assigned = list.assigned || {};
      status.textContent = fx.length ? (list.gpu ? "Shaders run on the GPU." : "") : "No transitions from packs yet. Import adds some.";
    } catch {
      fx = [];
      status.textContent = "This mixer has no fx library.";
    }
    paint();
  }

  async function send(files) {
    for (const f of files) {
      status.textContent = `Importing ${f.name}…`;
      try {
        const answer = await upload(client, f);
        const n = (answer.imported || []).length;
        toast({ text: answer.task_id ? `${f.name} is importing in the background` : `${f.name}: ${n} imported${(answer.skipped || []).length ? `, ${answer.skipped.length} skipped` : ""}` });
      } catch (e) {
        errorToast(e, "Import");
      }
    }
    refresh();
  }

  on(file, "change", () => send([...file.files]));
  on(sheet, "dragover", (e) => e.preventDefault());
  on(sheet, "drop", (e) => {
    e.preventDefault();
    if (client) send([...(e.dataTransfer?.files || [])]);
  });
  state.onChange(paint);
  return {
    el: sheet,
    fill() {
      filled = true;
      paint();
      refresh();
    },
    empty() {
      filled = false;
      body.replaceChildren();
    },
  };
}
