// The pictures on the welcome tiles.
//
// Drawn here rather than fetched: they are five small SVG strings, they cost no
// request, they inherit the theme's colours through `currentColor` and the two
// custom properties below, and there is no photograph of somebody else's church
// anywhere in the product.
//
// Each one is a 16:9 sketch of what the preset gives you, in the same visual
// language as the rest of the page: a rounded rectangle is a picture, a filled
// bar is a piece of furniture, the red rectangle is what is on air.

const OPEN = '<svg viewBox="0 0 160 90" width="100%" role="img" aria-hidden="true">';
const CLOSE = "</svg>";

/** A frame every tile starts with: the canvas, and the on air outline. */
function frame(inner, live) {
  const air = live
    ? '<rect x="4" y="4" width="152" height="82" rx="6" fill="none" stroke="var(--live)" stroke-width="2"/>'
    : "";
  return (
    OPEN +
    '<rect x="4" y="4" width="152" height="82" rx="6" fill="var(--panel2)"/>' +
    inner +
    air +
    CLOSE
  );
}

const CHURCH = frame(
  // A wide shot of a room, a lectern, and a band of lyrics across the bottom.
  '<rect x="12" y="12" width="136" height="50" rx="3" fill="var(--raise)"/>' +
    '<path d="M62 62V34l18-12 18 12v28z" fill="var(--panel)"/>' +
    '<path d="M80 26v10M75 31h10" stroke="var(--accent)" stroke-width="2" fill="none"/>' +
    '<rect x="26" y="46" width="14" height="16" rx="2" fill="var(--panel)"/>' +
    '<rect x="120" y="46" width="14" height="16" rx="2" fill="var(--panel)"/>' +
    '<rect x="12" y="66" width="136" height="12" rx="2" fill="var(--accent)" opacity="0.22"/>' +
    '<rect x="20" y="70" width="70" height="4" rx="2" fill="var(--text)" opacity="0.8"/>' +
    '<rect x="96" y="70" width="34" height="4" rx="2" fill="var(--text)" opacity="0.5"/>',
  true
);

const CLASSROOM = frame(
  // A camera on the left, the shared screen on the right, both on at once.
  '<rect x="12" y="12" width="62" height="50" rx="3" fill="var(--raise)"/>' +
    '<circle cx="43" cy="34" r="9" fill="var(--panel)"/>' +
    '<path d="M28 56a15 15 0 0 1 30 0z" fill="var(--panel)"/>' +
    '<rect x="82" y="12" width="66" height="50" rx="3" fill="var(--panel)"/>' +
    '<rect x="90" y="20" width="50" height="4" rx="2" fill="var(--accent)" opacity="0.7"/>' +
    '<rect x="90" y="30" width="38" height="3" rx="1.5" fill="var(--text)" opacity="0.45"/>' +
    '<rect x="90" y="38" width="44" height="3" rx="1.5" fill="var(--text)" opacity="0.45"/>' +
    '<rect x="90" y="46" width="26" height="3" rx="1.5" fill="var(--text)" opacity="0.45"/>' +
    '<circle cx="18" cy="72" r="4" fill="var(--live)"/>' +
    '<rect x="28" y="70" width="40" height="4" rx="2" fill="var(--text)" opacity="0.6"/>',
  false
);

const STREAMER = frame(
  // A full bleed game with the caster's camera in the corner, and a chat rail.
  '<rect x="12" y="12" width="104" height="66" rx="3" fill="var(--raise)"/>' +
    '<path d="M34 58l18-24 12 15 9-10 17 19z" fill="var(--accent)" opacity="0.5"/>' +
    '<circle cx="46" cy="26" r="5" fill="var(--warn)" opacity="0.8"/>' +
    '<rect x="78" y="46" width="34" height="28" rx="3" fill="var(--panel)" stroke="var(--live)" stroke-width="1.5"/>' +
    '<circle cx="95" cy="57" r="5" fill="var(--raise)"/>' +
    '<path d="M87 71a8 8 0 0 1 16 0z" fill="var(--raise)"/>' +
    '<rect x="122" y="12" width="26" height="66" rx="3" fill="var(--panel)"/>' +
    '<rect x="127" y="20" width="16" height="3" rx="1.5" fill="var(--text)" opacity="0.4"/>' +
    '<rect x="127" y="28" width="12" height="3" rx="1.5" fill="var(--text)" opacity="0.4"/>' +
    '<rect x="127" y="36" width="16" height="3" rx="1.5" fill="var(--text)" opacity="0.4"/>' +
    '<rect x="127" y="44" width="9" height="3" rx="1.5" fill="var(--text)" opacity="0.4"/>',
  false
);

const EMPTY = frame(
  '<rect x="12" y="12" width="136" height="66" rx="3" fill="var(--raise)" stroke="var(--line)" stroke-width="1" stroke-dasharray="4 4"/>' +
    '<path d="M80 30v30M65 45h30" stroke="var(--dim)" stroke-width="2.5" stroke-linecap="round" fill="none"/>',
  false
);

const OBS = frame(
  // A collection on the left coming across into a collection on the right.
  '<rect x="12" y="18" width="52" height="54" rx="3" fill="var(--panel)"/>' +
    '<rect x="18" y="26" width="40" height="10" rx="2" fill="var(--raise)"/>' +
    '<rect x="18" y="40" width="40" height="10" rx="2" fill="var(--raise)"/>' +
    '<rect x="18" y="54" width="40" height="10" rx="2" fill="var(--raise)"/>' +
    '<path d="M72 45h16M84 39l6 6-6 6" stroke="var(--accent)" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>' +
    '<rect x="96" y="18" width="52" height="54" rx="3" fill="var(--panel)"/>' +
    '<rect x="102" y="26" width="40" height="10" rx="2" fill="var(--accent)" opacity="0.5"/>' +
    '<rect x="102" y="40" width="40" height="10" rx="2" fill="var(--accent)" opacity="0.35"/>' +
    '<rect x="102" y="54" width="40" height="10" rx="2" fill="var(--accent)" opacity="0.2"/>',
  false
);

export const ART = {
  church: CHURCH,
  classroom: CLASSROOM,
  esports: STREAMER,
  empty: EMPTY,
  obs: OBS,
};

/** The picture for one tile, as a node. Falls back to the empty sketch. */
export function art(id) {
  const node = document.createElement("div");
  node.className = "welcome-art";
  node.innerHTML = ART[id] || ART.empty;
  return node;
}
