// The text editor's fields, and the params each one writes.
//
// Pure functions and a table, nothing imported, so the tests reach them
// without a DOM. `text-editor.js` draws the table.

/** What a text and a ticker look like before anything is changed. */
const DEFAULTS = {
  "text/source": { text: "", font: "Sans", size: 48, weight: 600, italic: false, color: "#ffffff", outline: "", shadow: false, background: "#000000b3", padding: 24, radius: 12, align: "left" },
  "ticker/source": { items: [], text: "", font: "Sans", size: 40, weight: 600, italic: false, color: "#ffffff", outline: "", shadow: false, background: "#000000b3", padding: 12, radius: 0, align: "left", speed: 120, direction: "left", loop: true, separator: "   •   " },
};

export const WEIGHTS = [[400, "Regular"], [500, "Medium"], [600, "Semi-bold"], [700, "Bold"], [900, "Black"]];

/**
 * The fields, in the order they are drawn. `ticker` marks the ones only a
 * ticker has; `kind` says which control draws it.
 */
export const FIELDS = [
  { key: "words", label: "Words", kind: "words" },
  { key: "font", label: "Font", kind: "text" },
  { key: "size", label: "Letter height", kind: "number", unit: "px", min: 4, max: 1000 },
  { key: "weight", label: "Weight", kind: "weight" },
  { key: "italic", label: "Italic", kind: "check" },
  { key: "color", label: "Colour", kind: "colour" },
  { key: "outline", label: "Outline", kind: "optional-colour", off: "#000000" },
  { key: "shadow", label: "Shadow", kind: "check" },
  { key: "background", label: "Box", kind: "box" },
  { key: "padding", label: "Padding", kind: "number", unit: "px", min: 0, max: 400 },
  { key: "radius", label: "Corners", kind: "number", unit: "px", min: 0, max: 400 },
  { key: "align", label: "Align", kind: "choice", options: [["left", "Left"], ["center", "Centre"], ["right", "Right"]] },
  { key: "speed", label: "Speed", kind: "number", unit: "px/s", min: 0, max: 2000, ticker: true },
  { key: "direction", label: "Direction", kind: "choice", options: [["left", "Right to left"], ["right", "Left to right"], ["up", "Rolling up"]], ticker: true },
  { key: "loop", label: "Go round again", kind: "check", ticker: true },
  { key: "separator", label: "Between items", kind: "text", ticker: true },
];

/** The fields a source of this type shows. */
export function fieldsFor(type) {
  return FIELDS.filter((f) => !f.ticker || type === "ticker/source");
}

/** What the editor opens on: the defaults with the source's own params over them. */
export function current(type, params) {
  const values = { ...(DEFAULTS[type] || DEFAULTS["text/source"]), ...(params || {}) };
  values.words = type === "ticker/source" ? (values.items && values.items.length ? values.items : [values.text]).join("\n") : values.text;
  return values;
}

/** The params one field's value writes. */
export function paramsFor(type, key, value) {
  if (key === "words") {
    return type === "ticker/source" ? { items: String(value).split("\n").filter((l, i, all) => l || i < all.length - 1) } : { text: String(value) };
  }
  return { [key]: value };
}

/** `#rrggbb` and an opacity 0 to 1 as `#rrggbbaa`. */
export function withAlpha(hex, opacity) {
  const a = Math.round(Math.max(0, Math.min(1, opacity)) * 255).toString(16).padStart(2, "0");
  return `${String(hex).slice(0, 7)}${a}`;
}

/** `#rrggbbaa` as `#rrggbb` and an opacity. */
export function splitAlpha(colour) {
  const c = String(colour || "#000000");
  const a = c.length === 9 ? parseInt(c.slice(7, 9), 16) / 255 : 1;
  return { hex: c.slice(0, 7), opacity: Math.round(a * 100) / 100 };
}
