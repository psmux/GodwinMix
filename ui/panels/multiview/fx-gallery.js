// The fx library beside Take: transitions and effects from packs.
//
// Transitions are listed in the transition picker (transition-sheet.js), each
// with a moving preview: a strip of twelve frames the core made once, moved by
// a CSS step animation, so the page decodes nothing and the moving stops when
// the picker closes (a hidden element is not painted). Effects, the items
// that play over the programme without changing it, are one Effects button
// that opens a list, and Alt+1 to Alt+9 (the keyboard map has them; a Mac
// whose Option key types a symbol can rebind them in the shortcuts list).

import { el } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { register } from "../../shell/commands.js";
import { withShow } from "../../client/transport-rpc.js";
import { popover } from "./studio-popover.js";

export const ACCEPT = ".zip,.webm,.mov,.mp4,.m4v,.mkv,.gif,.png,.jpg,.jpeg,.tif,.tiff,.webp,.glsl,.frag";

/** A URL on the core, with the token, for an `<img>` or a CSS background. */
function coreUrl(client, path) {
  const t = client.transport || {};
  const u = new URL(path, t.base || location.href);
  if (t.token) u.searchParams.set("token", t.token);
  return withShow(u).toString();
}

/** One file to `POST /api/v1/fx/upload`, which imports it. */
export function upload(client, file) {
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
  strip.style.backgroundImage = `url("${coreUrl(client, item.preview)}")`;
  const what = [item.kind, item.blend !== "normal" ? item.blend : "", item.runs || ""].filter(Boolean).join(" · ");
  return el("div.fx-tile", { title: item.note || what }, [strip, el("strong.ellipsis", { text: item.title || item.name }), el("small", { text: what }), el("div.fx-actions", {}, actions)]);
}

/**
 * The Effects button and its list, and their keys. Answers the button, the
 * list and a removal. The button stays hidden on a mixer with no effects.
 */
export function effects(client) {
  const list = el("div.fx-effect-list", { role: "group", "aria-label": "Effects" });
  const sheet = el("div.fx-effects-sheet", { "data-width": "320", role: "dialog", "aria-label": "Effects" }, [
    el("div.fx-head", {}, [el("strong", { text: "Effects over the programme" }), el("span.grow"), el("button.btn", { type: "button", text: "Done", onclick: () => pop.close() })]),
    el("p.fx-status", { text: "Each plays over whatever is on air and goes when it ends." }),
    list,
  ]);
  const button = el("button.btn.fx-open", { type: "button", hidden: true, "aria-haspopup": "dialog", title: "Effects that play over the programme (Alt+1 to Alt+9)" }, [
    el("span", { text: "Effects" }), el("span.fx-count"),
  ]);
  const pop = popover(sheet, button);
  let offs = [];
  async function load() {
    offs.forEach((off) => off());
    offs = [];
    try {
      const answer = await client.call("fx.list", { role: "effect" });
      const items = (answer && answer.fx) || [];
      list.replaceChildren(...items.map((i, n) => {
        const fire = () => client.call("fx.fire", { name: i.name }).catch((e) => errorToast(e, "Effect"));
        const key = n < 9 ? `Alt+${n + 1}` : "";
        if (key) offs.push(register({ id: `fx.fire-${n + 1}`, title: `Play ${i.title || i.name} over the programme`, group: "Effects", key, run: fire }));
        return el("button.btn.fx-fire", { type: "button", title: "Play over the programme", onclick: fire }, [
          el("span.ellipsis", { text: i.title || i.name }), key ? el("kbd", { text: key }) : null,
        ]);
      }));
      button.hidden = !items.length;
      button.lastChild.textContent = items.length ? String(items.length) : "";
    } catch {
      button.hidden = true;
    }
  }
  load();
  return { button, sheet, reload: load, close: () => pop.close(), off: () => offs.forEach((off) => off()) };
}
