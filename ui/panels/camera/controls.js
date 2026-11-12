// The hand controls for one camera, built for camera properties and, smaller,
// for the Preview overlay: the round joystick and the zoom rocker move it by
// hand, the framing chips and the Track switch let it follow a person by
// itself. A framing chip turns tracking on when it is off.

import { el } from "../../shell/dom.js";
import { joystick } from "./joystick.js";
import { zoomRocker } from "./zoom.js";
import { STYLES } from "./words.js";

/** A switch: a checkbox that looks like one, with its label. */
export function switchControl(label, onchange) {
  const input = el("input", { type: "checkbox", role: "switch", onchange: () => onchange(input.checked) });
  return { input, root: el("label.cam-switch", {}, [input, el("span.cam-switch-track", { "aria-hidden": "true" }), el("span", { text: label })]) };
}

/**
 * `hub` is hub.js's, `camera()` the id the controls act on now, `drive` a
 * Drive, `act` runs a command and says what went wrong.
 */
export function controls(hub, camera, drive, act) {
  const stick = joystick(drive);
  const zoom = zoomRocker(drive);
  const track = switchControl("Track", (on) => act(() => hub.api.track(camera(), on)));
  track.root.title = "Follow a person by itself";
  const chips = STYLES.map(([style, label, tip]) =>
    el("button.btn.sm.cam-chip", { type: "button", text: label, title: tip, "aria-pressed": "false", onclick: () => frameAs(style) }));

  function frameAs(style) {
    const id = camera();
    if (!id) return;
    const t = hub.track.get(id);
    act(() => (t && t.on ? hub.api.framing(id, style) : hub.api.track(id, true, style)));
  }

  return {
    stick,
    zoom,
    track,
    chips: el("div.cam-chips", { role: "group", "aria-label": "Framing" }, chips),
    /** Bring the switch and the chips up to `t`, a tracking state. */
    sync(t) {
      t = t || {};
      track.input.checked = !!t.on;
      chips.forEach((c, i) => {
        const on = !!t.on && t.style === STYLES[i][0];
        c.setAttribute("aria-pressed", String(on));
        c.classList.toggle("on", on);
      });
    },
    release() {
      stick.release();
      zoom.release();
    },
  };
}
