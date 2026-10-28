// A channel's own picture settings over its profile's (video_overrides):
// the shape its source is shown at, its deinterlace and method, and how a
// picture of another shape is fitted. The channel editor has it as a part
// of its own, and the channel list's bulk bar sends the same for every
// ticked channel. Each choice left at "The profile's" is not sent, so all
// of them left alone mean the profile as it is.

import { el } from "../../shell/dom.js";
import { ASPECT, METHOD, overrideFields, overridesFrom } from "./picture.js";
import { choice, field } from "./form.js";

const OWN = ["", "The profile's"];
const ROWS = {
  aspect: [OWN, ...ASPECT],
  deinterlace: [OWN, ["auto", "Only pictures marked interlaced"], ["on", "Every picture"], ["off", "Off"]],
  deinterlace_method: [OWN, ...METHOD],
  fit: [OWN, ["pad", "Pad with bars"], ["crop", "Crop to fill"], ["stretch", "Stretch to fill"]],
};
const LABEL = { aspect: "Source shape", deinterlace: "Deinterlace", deinterlace_method: "Deinterlace method", fit: "Another shape" };
const HINT = {
  aspect: "For a broadcaster that flags its picture wrong.",
  deinterlace_method: "On the CPU whatever the method; the scale and the encode stay on the GPU.",
};

/** The override choices. `who` goes before each label, for the bulk bar's. */
export function overrideForm(o, who = "") {
  const f = overrideFields(o);
  const c = Object.fromEntries(Object.keys(ROWS).map((k) => [k, choice(ROWS[k], f[k], who ? `${who}${LABEL[k].toLowerCase()}` : LABEL[k])]));
  const fields = Object.fromEntries(Object.keys(ROWS).map((k) => [k, field(LABEL[k], c[k], who ? "" : HINT[k])]));
  const node = el("div.hs-grid", {}, Object.values(fields));
  const sync = () => { fields.deinterlace_method.hidden = c.deinterlace.value === "off"; };
  node.addEventListener("change", sync);
  sync();
  return {
    node,
    controls: c,
    read: () => overridesFrom(Object.fromEntries(Object.entries(c).map(([k, s]) => [k, s.value]))),
  };
}

/** The channel editor's part: the form under a line that says what it is. */
export function overridesPart(o) {
  const form = overrideForm(o);
  return {
    ...form,
    node: el("div.hs-overrides", {}, [
      el("p.hs-note.dim", { text: "This channel's own picture settings, over its profile's on every size it makes. Left at The profile's, the profile decides." }),
      form.node,
    ]),
  };
}

/** The channel list's bulk part: the same choices, Set and Clear. */
export function bulkOverrides() {
  const form = overrideForm(null, "Ticked channels' ");
  const node = el("details.hs-more.hs-bulkpic", {}, [
    el("summary", { text: "Picture overrides" }),
    form.node,
    el("div.hs-row", {}, [
      el("button.btn", { type: "button", "data-bulk": "pic", text: "Set picture overrides", title: "Each ticked channel takes these in place of any it had" }),
      el("button.btn", { type: "button", "data-bulk": "unpic", text: "Clear picture overrides", title: "Each ticked channel takes its profile as it is" }),
    ]),
  ]);
  return { node, read: form.read, controls: form.controls };
}
