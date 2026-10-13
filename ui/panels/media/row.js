// One file in the Media panel: its name, its tags as chips that filter,
// what it is, and its buttons. A right click or a long press opens the same
// things as a menu.

import { el, fmtBytes } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { confirmModal } from "../../shell/modal.js";
import { fmtPosition } from "../../shell/fader.js";
import { nameOf, folderLabel } from "./model.js";
import * as act from "./actions.js";

/**
 * host: { selected, picking, chosen: Set, showFolder, select(item), roll(),
 *         pick(item, on), tagFilter(tag), menu(item, x, y), client }
 */
export function row(host, item) {
  const converting = item.conversion && item.conversion.state === "running";
  const failed = item.conversion && item.conversion.state === "failed";
  const chosen = host.chosen.has(item.name);
  const stop = (fn) => (e) => {
    e.stopPropagation();
    fn(e);
  };
  return el("div.row.mx-row", {
    title: item.path + (failed ? "\n" + item.conversion.error : ""),
    "data-drag-media": item.name,
    "aria-selected": String(chosen),
    style: { cursor: "pointer", background: host.selected === item.path || chosen ? "var(--raise)" : "" },
    onclick: (e) => (host.picking || e.ctrlKey || e.metaKey ? host.pick(item, !chosen) : host.select(item)),
    ondblclick: () => host.picking || (host.select(item), host.roll()),
    oncontextmenu: (e) => {
      e.preventDefault();
      host.menu(item, e.clientX, e.clientY);
    },
  }, [
    host.picking ? el("input", { type: "checkbox", checked: chosen, "aria-label": `Choose ${nameOf(item.name)}`, onclick: stop(() => host.pick(item, !chosen)) }) : null,
    el("span.ellipsis.grow.mx-name", {}, [
      el("span", { text: nameOf(item.name) }),
      host.showFolder && item.folder ? el("span.sm.faint", { text: `  ${folderLabel(item.folder)}` }) : null,
    ]),
    ...(item.tags || []).slice(0, 4).map((t) => el("button.pill.mx-tag", { text: t, title: `Show files tagged ${t}`, onclick: stop(() => host.tagFilter(t)) })),
    item.web_safe || item.converted_path ? el("span.pill", { text: "web", style: { color: "var(--ok)" } }) : null,
    item.has_audio === false ? el("span.pill", { text: "mute", style: { color: "var(--warn)" } }) : null,
    failed ? el("span.pill", { text: "convert failed", style: { color: "var(--bad)" } }) : null,
    el("span.num.sm.faint", { text: item.duration_ms ? fmtPosition(item.duration_ms) : fmtBytes(item.size_bytes) }),
    converting
      ? el("span.sm.dim", { text: `converting ${Math.round((item.conversion.progress || 0) * 100)}%` })
      : el("button.btn.icon", { text: "Add as source", "data-nodrag": "", onclick: stop(() => act.addAsSource(host.client, item)) }),
    !item.web_safe && !item.converted_path && !converting
      ? el("button.btn.icon", { text: "Convert", title: (item.reasons || []).join("; "), onclick: stop((e) => {
          e.currentTarget.disabled = true;
          host.client.call("media.convert", { name: item.name }).catch((err) => errorToast(err, "Convert"));
        }) })
      : null,
    el("button.btn.icon.danger", { text: "×", title: "Delete", onclick: stop(() => remove(host.client, item)) }),
  ]);
}

export async function remove(client, item) {
  if (!(await confirmModal(`Delete "${nameOf(item.name)}" from the library?`, "Delete"))) return;
  client.call("media.remove", { name: item.name }).catch((err) => errorToast(err, "Delete"));
}
