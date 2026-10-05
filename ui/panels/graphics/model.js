// What the Graphics gallery shows, worked out from `gallery.list` with no
// DOM: the label on a card, the filters, the addresses of its pictures and
// files, and the prompt a person hands their AI agent. Kept apart so the
// tests read it without a page.

/** What a card calls an item: by what it is for, then by what it is. */
export function label(item) {
  const zone = {
    "lower-third": "Lower third",
    bug: "Bug",
    bottom: "Ticker",
    top: "Top strip",
    center: "Card",
  }[item.zone];
  if (item.kind === "set") return "Virtual set";
  if (item.kind === "ticker") return "Ticker";
  if (item.kind === "transition") return "Transition";
  if (item.kind === "effect") return "Effect";
  if (item.zone === "full") return item.kind === "template" ? "Title card" : "Background";
  return zone || { template: "Graphic", image: "Picture", clip: "Clip", html: "Web graphic", ograf: "OGraf graphic", text: "Text" }[item.kind] || "Graphic";
}

/** The small words under a card's name. */
export function badges(item) {
  const out = [];
  if (item.moves) out.push("Moves");
  if (item.transparent) out.push("Transparent");
  out.push({ shipped: "Shipped", agent: item.made_by ? `By ${item.made_by}` : "By an agent", uploaded: "Imported" }[item.origin] || "");
  if (item.placed && item.placed.length) out.push("In use");
  return out.filter(Boolean);
}

/** The filter chips along the top, each a test on an item. */
export const FILTERS = [
  { id: "all", label: "All", test: () => true },
  { id: "lower", label: "Lower thirds", test: (i) => i.zone === "lower-third" },
  { id: "full", label: "Backgrounds", test: (i) => i.zone === "full" && i.kind !== "set" },
  { id: "ticker", label: "Tickers", test: (i) => i.kind === "ticker" || i.zone === "bottom" },
  { id: "bug", label: "Bugs", test: (i) => i.zone === "bug" },
  { id: "set", label: "Sets", test: (i) => i.kind === "set" },
  { id: "moving", label: "Moving", test: (i) => i.moves },
  { id: "mine", label: "Mine", test: (i) => i.origin !== "shipped" },
];

/** Items a filter keeps and a typed search finds, in the order given. */
export function visible(items, filterId, query) {
  const f = FILTERS.find((x) => x.id === filterId) || FILTERS[0];
  const words = String(query || "").toLowerCase().split(/\s+/).filter(Boolean);
  return items.filter((i) => {
    if (!f.test(i)) return false;
    const hay = [i.id, i.name, i.kind, i.zone, label(i), i.description, ...(i.tags || [])].join(" ").toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}

/** Where the page reaches the mixer, with its token for an <img> or <video>. */
function address(client, path, params = {}) {
  const tr = (client && client.transport) || {};
  const u = new URL(path, tr.base || location.origin);
  for (const [k, v] of Object.entries(params)) if (v !== undefined && v !== null) u.searchParams.set(k, String(v));
  if (tr.token) u.searchParams.set("token", tr.token);
  const show = typeof location !== "undefined" ? new URLSearchParams(location.search).get("show") : null;
  if (show) u.searchParams.set("show", show);
  return u.toString();
}

/** A card's picture. `v` changes when the item does, so the browser asks again. */
export function previewUrl(client, item, width) {
  return address(client, `/api/v1/gallery/${encodeURIComponent(item.id)}/preview.jpg`, { width, v: version(item) });
}

/** A short number that changes whenever what the card shows does. */
export function version(item) {
  const text = `${item.saved || ""}|${item.name}|${item.zone}|${JSON.stringify(item.values || {})}`;
  let h = 0;
  for (let i = 0; i < text.length; i += 1) h = (h * 31 + text.charCodeAt(i)) >>> 0;
  return h.toString(36);
}

/** One of an item's own files: its moving preview, a clip. */
export function fileUrl(client, item, file) {
  return address(client, `/api/v1/gallery/${encodeURIComponent(item.id)}/files/${file.split("/").map(encodeURIComponent).join("/")}`);
}

/** Where a picked or dropped file goes. */
export function uploadUrl(client, name) {
  const tr = (client && client.transport) || {};
  const u = new URL("/api/v1/gallery/upload", tr.base || location.origin);
  u.searchParams.set("name", name);
  return u.toString();
}

/** What a card's main button does next, by what has happened to it. */
export function nextStep(item, state) {
  if (item.kind === "transition" || item.kind === "effect") return null;
  if (item.kind === "set") return state === "placed" ? { id: "show", label: "Take live" } : { id: "place", label: "Make scene" };
  if (state === "shown") return { id: "hide", label: "Take off" };
  if (state === "placed") return { id: "show", label: "Take live" };
  return { id: "place", label: "Add" };
}
