// The Plugins page's words and rules, with no DOM: which cards a filter
// keeps, what a card's numbers and badges say, and what its button does.
// Everything here is what `plugin.gallery` answered, read for a person.

/** The filter chips, each a test on a card. */
export const FILTERS = [
  { id: "all", label: "All", keeps: () => true },
  { id: "source", label: "Inputs", keeps: (p) => has(p, "source", "device") },
  { id: "output", label: "Outputs", keeps: (p) => has(p, "output") },
  { id: "graphic", label: "Graphics", keeps: (p) => has(p, "graphic", "panel") },
  { id: "transition", label: "Transitions", keeps: (p) => has(p, "transition") },
  { id: "filter", label: "Effects", keeps: (p) => has(p, "filter") },
  { id: "service", label: "Helpers", keeps: (p) => has(p, "service", "surface", "encoder") },
  { id: "installed", label: "Installed", keeps: (p) => !!p.installed },
];

const has = (p, ...kinds) => (p.kinds || []).some((k) => kinds.includes(k));

/** A kind in the words on a card. */
const KIND_WORD = {
  source: "Input", output: "Output", device: "Finds devices", filter: "Effect", transition: "Transition",
  service: "Helper", graphic: "Graphics", panel: "Panel", surface: "Control surface", encoder: "Encoder",
  preset: "Preset", collection: "Scenes",
};

export function kindWords(p) {
  return [...new Set((p.kinds || []).map((k) => KIND_WORD[k] || k))];
}

/** The cards a chip and a search keep, in the order the core sent them. */
export function visible(plugins, filter, query) {
  const chip = FILTERS.find((f) => f.id === filter) || FILTERS[0];
  const words = String(query || "").trim().toLowerCase().split(/\s+/).filter(Boolean);
  return plugins.filter((p) => {
    if (!chip.keeps(p)) return false;
    const hay = [p.name, p.title, p.description, p.author, ...(p.tags || []), ...kindWords(p)].join(" ").toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}

/** 1234 as "1.2k", so a count fits beside a star. */
export function count(n) {
  if (typeof n !== "number") return "";
  if (n < 1000) return String(n);
  if (n < 1e6) return `${(n / 1000).toFixed(n < 1e4 ? 1 : 0).replace(/\.0$/, "")}k`;
  return `${(n / 1e6).toFixed(1).replace(/\.0$/, "")}M`;
}

/** The numbers under a card's title. */
export function numbers(p) {
  const out = [];
  if (typeof p.stars === "number") out.push({ icon: "star", text: count(p.stars), title: `${p.stars} stars on GitHub` });
  if (typeof p.downloads === "number") out.push({ icon: "down", text: count(p.downloads), title: `Downloaded ${p.downloads} times` });
  return out;
}

/** The quality tier as a badge, and the sentence behind it. */
export function tier(p) {
  switch (p.tier) {
    case "gold": return { label: "Gold", why: "Maintained by the GodwinMix project and tested on every platform it lists." };
    case "silver": return { label: "Silver", why: "Tested on every platform it lists." };
    case "bronze": return { label: "Bronze", why: "Tested on at least one platform." };
    default: return { label: "Unreviewed", why: "Nobody has checked this one. It runs with the permissions you give it." };
  }
}

/** The first sentence of a description, for the card. */
export function oneLine(text) {
  const t = String(text || "").trim();
  const end = t.search(/[.:;](\s|$)/);
  return end > 0 ? t.slice(0, end + (t[end] === "." ? 1 : 0)) : t;
}

/**
 * The button a card or the detail view shows first, and what it does.
 * `act` is "add", "update", or null for a button that only says something.
 */
export function primary(p) {
  if (!p.runs_here && !p.installed) return { label: p.fit || "Not for this computer", act: null };
  if (p.update) return { label: "Update", act: "update" };
  if (p.installed) return { label: "Installed", act: null };
  return { label: "Install", act: "add" };
}

/** The line under the page title, saying how fresh the list is. */
export function freshness(gallery, now = Date.now()) {
  const markets = (gallery && gallery.marketplaces) || [];
  const fetched = Math.max(0, ...markets.map((m) => m.fetched || 0));
  if (!markets.length) return "No plugin list is set up on this mixer.";
  if (!fetched) return "The plugin list could not be fetched. Check the mixer can reach the internet, then try again.";
  const mins = Math.round((now / 1000 - fetched) / 60);
  const age = mins < 1 ? "just now" : mins < 60 ? `${mins} min ago` : mins < 1440 ? `${Math.round(mins / 60)} h ago` : `${Math.round(mins / 1440)} days ago`;
  return gallery.offline ? `Offline. Showing the list from ${age}.` : `Updated ${age}.`;
}
