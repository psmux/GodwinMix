// One custom rung's Advanced group in HLS for viewers: how a picture of
// another shape is fitted, the bar colour, deinterlacing, the encoder's
// preset and B frames. The same fields as the Custom form's Advanced group
// (custom-advanced.js), less the ones a ladder decides for every rung: the
// keyframes follow the segment length, and the sound is one track that every
// rung shares, set once for the ladder.

import { advancedFrom, advancedInto, advancedProps, advancedRules } from "./custom-advanced.js";

/** The fields a rung may set on its own. */
export const RUNG_FIELDS = ["fit", "pad_color", "deinterlace", "preset", "bframes"];

/** The rung's Advanced schema, grouped and closed as everywhere else. */
export function rungSchema() {
  const all = advancedProps();
  const properties = Object.fromEntries(RUNG_FIELDS.map((k) => [k, all[k]]));
  return { type: "object", properties, allOf: advancedRules };
}

/** The picture settings of `from` laid on `to`, a rung made again by a change of size or bitrate. */
export function keepAdvanced(to, from) {
  const v = (from && from.video) || {};
  for (const k of RUNG_FIELDS) if (v[k] !== undefined && v[k] !== null) to.video[k] = v[k];
  return to;
}

/** The form's answers written into the rung, in place: what is left at its default is taken out. */
export function intoRung(r, values) {
  for (const k of RUNG_FIELDS) delete r.video[k];
  // Only the rung's own fields go in: its sound and keyframes are the ladder's.
  const own = Object.fromEntries(RUNG_FIELDS.map((k) => [k, values[k]]));
  advancedInto({ video: r.video }, own);
  return r;
}

/** True when the rung asks for anything its Advanced group sets. */
export function rungUsesAdvanced(r) {
  const v = (r && r.video) || {};
  return (v.fit && v.fit !== "pad") || (v.deinterlace && v.deinterlace !== "off") || !!v.preset || v.bframes !== undefined || !!v.pad_color;
}

/**
 * The form, built by the schema kit on first use, under the rung's row.
 * `onChange` gets the rung after every change.
 */
export async function rungAdvanced(r, onChange) {
  const { SchemaInspector } = await import("../../kits/schema/index.js");
  const form = new SchemaInspector({
    schema: rungSchema(),
    value: advancedFrom(r),
    onChange: (values) => onChange(intoRung(r, values)),
  });
  form.el.classList.add("rnd-rungadv");
  const box = form.el.querySelector("details");
  if (box) {
    box.querySelector("summary").textContent = `Advanced, ${r.video.height}p`;
    if (rungUsesAdvanced(r)) box.open = true;
  }
  return form.el;
}
