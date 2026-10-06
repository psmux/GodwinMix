// A small drawing of each built in transition: two boxes, the old scene and
// the new one. Still beside Take, where it shows the shape half way through;
// moving in the picker, where studio.css walks the new box through the whole
// transition for as long as the picker is open and not a moment longer (a
// closed picker is not painted, so nothing is drawn while it is shut).

const OLD = 'class="o" fill="currentColor" opacity="0.35"';
const NEW = 'class="n" fill="currentColor"';
const FULL = 'x="1" y="1" width="26" height="14"';

const SHAPES = {
  fade: `<rect ${FULL} ${OLD}/><rect ${FULL} ${NEW} opacity="0.6"/>`,
  move: `<rect ${FULL} ${OLD}/><rect x="9" y="4" width="14" height="9" ${NEW}/>`,
  wipe: `<rect ${FULL} ${OLD}/><rect x="14" y="1" width="13" height="14" ${NEW}/>`,
  slide: `<rect ${FULL} ${OLD}/><rect x="14" y="1" width="26" height="14" ${NEW}/>`,
  push: `<rect x="-12" y="1" width="26" height="14" ${OLD}/><rect x="14" y="1" width="26" height="14" ${NEW}/>`,
  zoom: `<rect ${FULL} ${OLD}/><rect x="8" y="4" width="12" height="8" ${NEW}/>`,
  "zoom-out": `<rect ${FULL} ${NEW}/><rect x="8" y="4" width="12" height="8" ${OLD}/>`,
  box: `<rect ${FULL} ${OLD}/><rect x="7" y="4" width="14" height="8" ${NEW}/>`,
  dip: `<rect ${FULL} ${OLD}/><rect ${FULL} class="n" fill="#000" opacity="0.8"/>`,
};

/** A drawing that names a transition the core added: its first letter. */
function lettered(name) {
  const letter = String(name || "?").trim().charAt(0).toUpperCase() || "?";
  return `<rect ${FULL} ${OLD}/><text x="14" y="12" text-anchor="middle" font-size="10" font-weight="700" fill="currentColor">${letter.replace(/[<&>"]/g, "")}</text>`;
}

/** The SVG for a transition; `moving` marks it for the picker's animation. */
export function iconSvg(type, moving = false) {
  const body = SHAPES[type] || lettered(type);
  const cls = moving && SHAPES[type] ? ` class="tp-moving" data-type="${type}"` : "";
  return `<svg viewBox="0 0 28 16" width="28" height="16" aria-hidden="true"${cls}>${body}</svg>`;
}

export function hasDrawing(type) {
  return Boolean(SHAPES[type]);
}
