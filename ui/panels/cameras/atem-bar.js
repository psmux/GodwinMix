// The ATEM bar, shown when a switcher is connected: a dot that says whether
// the link is up, which switcher (a picker when there are two), its
// programme and preview rows, the downstream keys and fade to black.

import { el } from "../../shell/dom.js";
import { bus, plainInputs } from "./bus.js";
import { MODE_WORDS } from "./words.js";

export function atemBar(model, act) {
  const dot = el("span.dot");
  const name = el("span.cam-atem-name");
  const picker = el("select.cam-atem-pick", { "aria-label": "Switcher", onchange: () => model.pickAtem(picker.value) });
  const mode = el("span.cam-atem-mode.dim");
  const sw = () => model.atemId;
  const pgm = bus("Programme", "pgm", (i) => act(() => model.api.atemProgram(sw(), i)));
  const pvw = bus("Preview", "pvw", (i) => act(() => model.api.atemPreview(sw(), i)));
  const dsks = el("div.cam-atem-keys");
  const ftb = el("button.cam-key.ftb", { type: "button", text: "FTB", title: "Fade to black", onclick: () => act(() => model.api.atemFtb(sw())) });
  const root = el("section.cam-atem", { "aria-label": "Switcher" }, [
    el("div.cam-atem-head", {}, [dot, name, picker, mode]),
    el("div.cam-atem-buses", {}, [pgm.root, pvw.root, el("div.cam-atem-side", {}, [dsks, ftb])]),
  ]);
  let dskKey = "";

  function keys(atem) {
    const k = (atem.dsks || []).length + sw();
    if (k !== dskKey) {
      dskKey = k;
      dsks.replaceChildren(...(atem.dsks || []).map((_, i) =>
        el("button.cam-key.dsk", { type: "button", text: `DSK ${i + 1}`, title: `Downstream key ${i + 1}, on or off with its transition`, onclick: () => act(() => model.api.atemDsk(sw(), i, "auto")) })));
    }
    [...dsks.children].forEach((b, i) => {
      b.classList.toggle("on", !!atem.dsks[i].on);
      b.setAttribute("aria-pressed", String(!!atem.dsks[i].on));
    });
  }

  return {
    root,
    sync() {
      const atem = model.atem;
      root.hidden = !model.atems.length;
      if (root.hidden) return;
      const listed = model.atems.find((a) => a.id === model.atemId) || model.atems[0];
      const up = !!(atem ? atem.connected : listed.connected);
      dot.className = "dot " + (up ? "live" : "failed");
      dot.title = up ? "Connected" : "Not connected";
      name.textContent = (atem || listed).name || listed.id;
      name.hidden = model.atems.length > 1;
      picker.hidden = model.atems.length < 2;
      if (picker.options.length !== model.atems.length) picker.replaceChildren(...model.atems.map((a) => el("option", { value: a.id, text: a.name || a.id })));
      picker.value = model.atemId;
      mode.textContent = up ? MODE_WORDS[(atem || listed).mode] || "" : "Not connected. Check the switcher's network cable and address.";
      root.classList.toggle("down", !up);
      if (!atem || !atem.mes) return;
      const me = atem.mes[0];
      const inputs = model.pro ? atem.inputs : plainInputs(atem);
      pgm.show(inputs, me.program);
      pvw.show(inputs, me.preview);
      keys(atem);
      ftb.classList.toggle("on", !!(atem.ftb && atem.ftb.on));
    },
  };
}
