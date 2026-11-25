// What camera properties, the Preview overlay and the switcher say, in words
// a volunteer understands.

export const STYLES = [
  ["close", "Close", "Head and shoulders"],
  ["waist", "Waist", "From the waist up"],
  ["full", "Full", "The whole person"],
  ["wide", "Wide", "The person and the space around them"],
];

const STYLE_WORDS = { close: "close up", waist: "waist up", full: "full length", wide: "wide" };

/** "Following: full length", from a track/state answer. */
export function trackWords(t) {
  if (t && t.state === "error") return "No picture to track. Check the camera's source";
  if (!t || !t.on) return "Tracking off";
  switch (t.state) {
    case "following":
      return `Following: ${STYLE_WORDS[t.style] || t.style || "waist up"}`;
    case "lost":
      return "Lost them, holding the shot";
    default:
      return "Looking for someone";
  }
}

/** The tile's badge: a word only while the camera is doing something by itself. */
export function badgeWords(t) {
  if (!t || !t.on) return "";
  return { following: "Following", lost: "Holding", error: "No picture" }[t.state] || "Looking";
}

/** The ATEM's input a source is mapped to, or null. */
export function inputOf(atem, source) {
  return atem && atem.map ? atem.map[source] ?? null : null;
}

/** An input's short name ("CAM1"), for the Programme header's chip. */
export function shortOf(atem, id) {
  const i = atem && (atem.inputs || []).find((x) => x.id === id);
  return i ? i.short || i.name : id === undefined || id === null ? "" : String(id);
}

export const MODES = [
  ["own", "On its own", "Driven from here and by agents. A GodwinMix take leaves it alone."],
  ["follow", "Follows GodwinMix", "A GodwinMix take cuts the switcher to the same camera."],
  ["lead", "Leads GodwinMix", "A cut on the switcher's own panel moves GodwinMix."],
];
