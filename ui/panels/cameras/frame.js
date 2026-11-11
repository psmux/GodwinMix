// Step two: frame the camera that is next. The joystick and zoom rocker
// move it by hand; the framing chips and the Track switch let it follow a
// person by itself; the saved shots and the exposure drawer sit below.

import { el } from "../../shell/dom.js";
import { joystick } from "./joystick.js";
import { zoomRocker } from "./zoom.js";
import { presetGrid } from "./presets.js";
import { drawer, switchControl } from "./drawer.js";
import { STYLES } from "./words.js";

export function frameStep(model, drive, act) {
  const title = el("span.cam-framing-name");
  const stick = joystick(drive);
  const zoom = zoomRocker(drive);
  const track = switchControl("Track a person", (on) => act(() => model.api.track(model.current.id, on)));
  const chips = STYLES.map(([style, label, tip]) =>
    el("button.cam-chip", { type: "button", text: label, title: tip, "aria-pressed": "false", onclick: () => frameAs(style) }));
  const local = new Map();
  const remember = (id, r) => r && local.set(id, { ...(local.get(id) || {}), ...r });
  const presets = presetGrid({
    recall: (p) => act(() => model.api.presetRecall(model.current.id, p.slot)),
    save: (slot) => act(async () => {
      const id = model.current.id;
      await model.api.presetSave(id, slot, `Shot ${slot + 1}`);
      await model.loadPresets(id);
    }),
  });
  const more = drawer({
    exposure: (e) => act(async () => remember(model.current.id, await model.api.exposure(model.current.id, e))),
    focus: (f) => act(async () => remember(model.current.id, await model.api.focus(model.current.id, f))),
  });

  function frameAs(style) {
    const cam = model.current;
    if (!cam) return;
    act(async () => {
      await model.api.framing(cam.id, style);
      const t = model.track.get(cam.id);
      if (!t || !t.on) await model.api.track(cam.id, true);
    });
  }

  const root = el("section.cam-step.cam-step-frame", {}, [
    el("h3.cam-steptitle", {}, [el("span.cam-stepno", { text: "2" }), "Frame it", title]),
    el("div.cam-frame-grid", {}, [
      el("div.cam-frame-move", {}, [stick.root, zoom.root]),
      el("div.cam-frame-auto", {}, [
        track.root,
        el("div.cam-chips", { role: "group", "aria-label": "Framing" }, chips),
        el("p.cam-hint.dim", { text: "Tap a person in the picture to follow them." }),
      ]),
    ]),
    presets.root,
    more.root,
  ]);

  return {
    root,
    stick,
    zoom,
    sync() {
      const cam = model.current;
      root.classList.toggle("none", !cam);
      title.textContent = cam ? cam.name || cam.id : "";
      drive.use(cam ? cam.id : null);
      const t = (cam && model.track.get(cam.id)) || {};
      track.input.checked = !!t.on;
      chips.forEach((c, i) => {
        const on = !!t.on && t.style === STYLES[i][0];
        c.setAttribute("aria-pressed", String(on));
        c.classList.toggle("on", on);
      });
      presets.show(cam, cam && model.presets.get(cam.id));
      more.show(cam && { ...cam, ...(local.get(cam.id) || {}) });
    },
    release() {
      stick.release();
      zoom.release();
    },
  };
}
