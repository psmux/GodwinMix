// Graphic templates, as the picker and the settings drawer see them.
//
// A template is an SVG with named fields that the mixer draws itself
// (`template/source`). `template.list` names each one with its fields, and a
// field's value lives at `params.fields.<name>` on the source. These are the
// pure parts: what to send, and where the item goes. Nothing imported, so the
// tests reach them without a DOM.

/** Whether a source is a graphic drawn from a template. */
export function isGraphic(source) {
  return String((source && source.type) || "") === "template/source";
}

/** A colour as the `#rrggbb` a colour input takes, dropping any alpha. */
export function colourInput(value) {
  const v = String(value || "");
  if (/^#[0-9a-f]{3}$/i.test(v)) return "#" + [...v.slice(1)].map((c) => c + c).join("");
  return /^#[0-9a-f]{6}/i.test(v) ? v.slice(0, 7).toLowerCase() : "#000000";
}

/** Text fields first, then colours, in the order the template declares them. */
export function ordered(fields) {
  const list = fields || [];
  return list.filter((f) => f.type !== "color").concat(list.filter((f) => f.type === "color"));
}

/**
 * Only the values that differ from what the form opened on. A colour left
 * alone is not sent, so the station's brand colours keep applying to it.
 */
export function changedOnly(shown, values) {
  const out = {};
  for (const [name, value] of Object.entries(values || {})) {
    if (value !== (shown || {})[name]) out[name] = value;
  }
  return out;
}

/** The `source.add` request for a template and the values typed for it. */
export function addRequest(template, fields) {
  const req = { uri: template.uri, name: template.title || template.name, type: "template/source" };
  if (fields && Object.keys(fields).length) req.params = { fields };
  return req;
}

/**
 * Over the whole canvas, keeping the template's shape. Every pack template is
 * drawn on a full 1920 by 1080 frame with its graphic inside title safe, so
 * this puts a lower third where a lower third belongs on any 16:9 canvas.
 */
export function placementFor(canvas) {
  return { position: { x: 0, y: 0 }, frame: { w: Math.round(canvas.width), h: Math.round(canvas.height) }, fit: "contain", align: "center" };
}

/** The `source.set` request that changes some fields and leaves the rest. */
export function setRequest(id, fields) {
  return { id, params: { fields } };
}
