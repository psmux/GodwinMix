// A profile's HLS ladder in the custom form: further sizes of the same
// picture, each a variant of its own in the master playlist beside the
// main one, so a player on a weak connection can step down. Every rung
// shares the sound. A rung is a size and a bitrate; its frame rate and
// preset are the main picture's unless it says otherwise. Pushes and
// other outputs carry the main picture only.

import { el } from "../../shell/dom.js";
import { SIZES } from "./conv-fields.js";
import { FPS, nextRung, rungFields, rungFrom, withValue } from "./picture.js";
import { choice, field, num, text } from "./form.js";

const RUNG_FPS = [["", "The main picture's"], ...FPS.slice(1)];

/**
 * @param {object[]} rungs the profile's ladder
 * @param {() => object|null} main the main picture as the form has it now
 */
export function ladderBox(rungs, main) {
  const rows = el("div.hs-rungs");
  const add = (r) => {
    const f = rungFields(r);
    const sizes = withValue(SIZES.filter(([v]) => v !== "source"), f.size, f.size.replace("x", "×"));
    const c = {
      size: choice(sizes, f.size, "Rung size"),
      bitrate_kbps: num(f.bitrate_kbps, "1500", "Rung bitrate", { min: "100", step: "100" }),
      fps: choice(withValue(RUNG_FPS, f.fps), f.fps, "Rung frame rate"),
      preset: text(f.preset, "The main picture's", "Rung preset"),
    };
    const del = el("button.btn.icon", { type: "button", text: "×", title: "Remove this size", "aria-label": "Remove this size" });
    const row = el("div.hs-rung", {}, [field("Size", c.size), field("Bitrate, kb/s", c.bitrate_kbps), field("Frame rate", c.fps), field("Preset", c.preset), del]);
    row.read = () => rungFrom(Object.fromEntries(Object.entries(c).map(([k, input]) => [k, input.value])), r);
    del.onclick = () => row.remove();
    rows.append(row);
    return row;
  };
  for (const r of rungs || []) add(r);
  const read = () => [...rows.children].map((row) => row.read());
  const more = el("button.btn", { type: "button", text: "Add a size", onclick: () => add(nextRung(main(), read())).querySelector("select").focus() });
  return {
    node: el("div.hs-ladder", {}, [
      el("p.hs-note.dim", { text: "Further sizes for the HLS output, each one a variant a player can step down to. They share the sound, and what a size leaves empty is the main picture's. Pushes carry the main picture only." }),
      rows, el("div.hs-row", {}, [more]),
    ]),
    add,
    read,
  };
}
