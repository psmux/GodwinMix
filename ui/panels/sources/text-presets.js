// Text and tickers, ready to go on a scene in one press.
//
// Each preset is a `source.add` request and the place its item takes in the
// scene, in canvas pixels worked out from the canvas the mixer reports. The
// words are placeholders an operator overtypes in the source's settings,
// which apply on air as they are typed.
//
// Its own module with nothing imported, so the tests reach it without the
// shell behind it.

/** A frame of the canvas, as a scene item transform. */
function frame(x, y, w, h, fit, align) {
  return { position: { x: Math.round(x), y: Math.round(y) }, frame: { w: Math.round(w), h: Math.round(h) }, fit, align };
}

export const TEXT_PRESETS = [
  {
    key: "lower-third",
    name: "Lower third",
    note: "A name and a title in a box, bottom left",
    type: "text/source",
    params: { text: "Name Surname\nTitle", size: 44, weight: 700, background: "#000000b3", padding: 24, radius: 12 },
    place: (w, h) => frame(w * 0.05, h * 0.7, w * 0.6, h * 0.18, "contain", "center-left"),
  },
  {
    key: "ticker",
    name: "Ticker",
    note: "Words crawling right to left along the bottom",
    type: "ticker/source",
    params: { items: ["Your first item", "The next one"], size: 34, background: "#101010e6", speed: 120 },
    place: (w, h) => frame(0, h - h * 0.07, w, h * 0.07, "stretch"),
  },
  {
    key: "title",
    name: "Title",
    note: "Large words across the middle, with no box",
    type: "text/source",
    params: { text: "Title", size: 96, weight: 800, background: "", shadow: true, align: "center" },
    place: (w, h) => frame(w * 0.1, h * 0.35, w * 0.8, h * 0.3, "contain", "center"),
  },
  {
    key: "credits",
    name: "Credits roll",
    note: "Lines rolling up the whole screen",
    type: "ticker/source",
    params: { items: ["Director", "Name Surname", "", "Camera", "Name Surname"], direction: "up", speed: 60, size: 40, background: "", align: "center" },
    place: (w, h) => frame(w * 0.2, 0, w * 0.6, h, "stretch"),
  },
];

/** The `source.add` request for a preset. */
export function addRequest(preset) {
  const scheme = preset.type === "ticker/source" ? "ticker:" : "text:";
  return { uri: scheme, name: preset.name, type: preset.type, params: JSON.parse(JSON.stringify(preset.params)) };
}

/** The canvas the mixer draws, or a 1080p guess when it does not say. */
export async function canvasOf(client) {
  const info = await client.call("core.info").catch(() => null);
  const c = (info && info.canvas) || {};
  return { width: c.width || 1920, height: c.height || 1080 };
}

/** Where a preset's item goes on a canvas of this size. */
export function placementFor(preset, canvas) {
  return preset.place(canvas.width, canvas.height);
}

/** Whether a source is one the text editor opens for. */
export function isText(source) {
  return ["text/source", "ticker/source"].includes(String((source && source.type) || ""));
}
