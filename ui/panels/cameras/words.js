// What the camera window says, in words a volunteer understands, and which
// camera is on air or next whichever box made the cut.

export const STYLES = [
  ["close", "Close", "Head and shoulders"],
  ["waist", "Waist", "From the waist up"],
  ["full", "Full", "The whole person"],
  ["wide", "Wide", "The person and the space around them"],
];

const STYLE_WORDS = { close: "close up", waist: "waist up", full: "full length", wide: "wide" };

/** "Following: full length", from a track/state answer. */
export function trackWords(t) {
  if (!t || !t.on) return "Tracking off";
  switch (t.state) {
    case "following":
      return `Following: ${STYLE_WORDS[t.style] || t.style || "waist up"}`;
    case "manual":
      return "Steering by hand";
    case "holding":
      return "Holding the spot you tapped";
    case "lost":
      return "Lost them, holding the shot";
    default:
      return "Looking for someone";
  }
}

/** The ATEM's input a camera is mapped to, or null. */
export function inputOf(atem, cam) {
  return atem && atem.map ? atem.map[cam.source] ?? atem.map[cam.id] ?? null : null;
}

/** "program", "preview" or "" for a camera, from GodwinMix's tally and the ATEM's buses. */
export function tallyOf(cam, mix, atem, next) {
  const t = (mix && mix.tally && mix.tally[cam.source]) || "";
  const input = inputOf(atem, cam);
  const me = atem && atem.mes && atem.mes[0];
  if (t === "program" || (me && input !== null && me.program === input)) return "program";
  if (next === cam.id || t === "preview" || (me && input !== null && me.preview === input)) return "preview";
  return "";
}

/** Where a take goes for a switcher's mode, and the line that says so. */
export function route(atem, pick) {
  if (!atem || !atem.connected) return { mix: true, atem: false, words: "GodwinMix" };
  if (atem.mode === "follow") return { mix: true, atem: false, words: `GodwinMix, and ${atem.name} follows` };
  if (atem.mode === "lead") return { mix: false, atem: true, words: `${atem.name}, and GodwinMix follows` };
  const to = pick || "atem";
  return { mix: to !== "atem", atem: to !== "mix", words: { mix: "GodwinMix only", atem: `${atem.name} only`, both: "GodwinMix and the switcher" }[to], pick: to };
}

export const MODE_WORDS = {
  own: "On its own: this window drives it",
  follow: "Follows GodwinMix: one take cuts both",
  lead: "Leads: its panel moves GodwinMix",
};
