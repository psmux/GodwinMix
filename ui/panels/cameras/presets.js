// The preset grid: each saved shot as its picture, tap to go there, and a
// plus tile that saves where the camera points now as the next shot.

import { el } from "../../shell/dom.js";

export function presetGrid(on) {
  const grid = el("div.cam-presets", { role: "group", "aria-label": "Saved shots" });
  const root = el("div.cam-presetwrap", {}, [el("div.cam-label", { text: "Saved shots" }), grid]);
  let key = "";

  function tile(p) {
    const pic = p.thumbnail ? el("img", { src: p.thumbnail, alt: "", draggable: "false" }) : el("div.cam-nopic", { text: String(p.slot) });
    return el("button.cam-preset", { type: "button", title: `Go to ${p.name}`, onclick: () => on.recall(p) }, [
      pic,
      el("span.cam-preset-name", { text: p.name || `Shot ${p.slot}` }),
    ]);
  }

  return {
    root,
    show(cam, presets) {
      const list = presets || [];
      const next = list.reduce((m, p) => Math.max(m, p.slot + 1), 1);
      const k = (cam ? cam.id : "") + list.map((p) => p.slot + p.name + (p.thumbnail || "").length).join();
      if (k === key) return;
      key = k;
      const add = el("button.cam-preset.cam-preset-add", { type: "button", title: "Save this shot", "aria-label": "Save this shot", onclick: () => on.save(next) }, [
        el("span.cam-plus", { text: "+" }),
        el("span.cam-preset-name", { text: "Save shot" }),
      ]);
      grid.replaceChildren(...list.map(tile), ...(cam ? [add] : []));
    },
  };
}
