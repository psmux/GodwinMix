// The prompts behind "Make one with an AI agent": one per kind of graphic,
// ready to paste into Claude Code, opencode, pi or any other agent. Each
// names the tools it should call and the loop it should run (save, look,
// fix, save again), because a small free model follows a plan it is given
// far better than one it has to invent.

const LOOP =
  "Save it with save_graphic (give it a clear name, tags and a one line description), then look at it with " +
  "preview_graphic and fix anything cut off, too small to read or outside the safe area. Save again with " +
  "replace: true until it looks right, then tell me its id. If you have no MCP tools, run the same tools from " +
  "a shell as: godwinmix tool save_graphic '<json>'. The godwinmix-design skill has the rules.";

export const KINDS = [
  {
    id: "lower-third",
    label: "Lower third",
    ask: "a lower third for a person's name and their role",
    how: "Make it an SVG template: a 1920 by 1080 canvas, transparent everywhere but the strap, with {{name}} and {{title}} fields and data-fit-width on each text, inside title safe (x 96 to 1824, y 54 to 1026), zone lower-third.",
  },
  {
    id: "ticker",
    label: "Ticker",
    ask: "a news ticker that crawls along the bottom",
    how: 'Save it as source {"uri": "ticker:", "params": {"items": [...], "speed": 120}} with a bar colour that matches the show, zone bottom.',
  },
  {
    id: "background",
    label: "Background",
    ask: "a full screen background to sit behind a presenter or a title",
    how: "Make it an SVG with no fields, 1920 by 1080, filling the frame edge to edge, zone full. Keep it quiet enough for words to read over it.",
  },
  {
    id: "set",
    label: "Virtual set",
    ask: "a virtual studio set for a presenter standing in front of a green screen",
    how: 'Make a background plate (1920 by 1080 PNG or SVG) and, if it suits, a transparent foreground such as a desk, then save them together with set: {"background": ..., "foreground": ..., "settings": {"presenter_scale": 0.8, "presenter_x": 0.62}}.',
  },
  {
    id: "bug",
    label: "Bug",
    ask: "a corner bug with the channel's logo and a short label",
    how: "Make it an SVG template on a 1920 by 1080 transparent canvas with the bug in the top right inside title safe, a {{label}} field, zone bug.",
  },
  {
    id: "title-card",
    label: "Title card",
    ask: "a full screen title card to open the show",
    how: "Make it an SVG template, 1920 by 1080, opaque, with {{title}} and {{subtitle}} fields and data-fit-width on each, zone full.",
  },
];

/** The whole prompt for one kind, with what the person wrote about the show. */
export function promptFor(kindId, about) {
  const k = KINDS.find((x) => x.id === kindId) || KINDS[0];
  const show = String(about || "").trim();
  const forShow = show ? ` for this show: ${show}.` : ". Ask me one short question about the show's look first if you need to.";
  return `Design ${k.ask} in GodwinMix${forShow}\n\n${k.how}\n\n${LOOP}`;
}
