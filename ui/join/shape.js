// The shape a phone sends, and which way up. Arithmetic only; `shaper.js`
// draws with it.
//
// A tester connected a phone from the QR code and found the picture followed
// the phone's auto rotate setting: held sideways with auto rotate off, the
// mixer got a sideways picture, and turning the phone on the air changed the
// shape under the mixer's feet. So the person picks the shape before going
// live, the page holds it whatever the phone does, and the motion sensor
// turns the picture upright when the screen did not turn with the phone.
//
// Turns are clockwise quarter turns in degrees: 0, 90, 180, 270.

/** What each shape sends. The same pixel count as 720p, so the bitrate holds. */
export const SHAPES = {
  landscape: { w: 1280, h: 720, label: "Landscape", hint: "16:9, for a normal stream" },
  portrait: { w: 720, h: 1280, label: "Portrait", hint: "9:16, for Shorts, Reels and TikTok" },
  square: { w: 720, h: 720, label: "Square", hint: "1:1" },
};

export const FITS = {
  fill: { label: "Fill", hint: "Fill the frame, cutting off the edges" },
  fit: { label: "Fit", hint: "Show the whole picture, with bars" },
};

export const isShape = (s) => Object.prototype.hasOwnProperty.call(SHAPES, s);

/** The shape a mixer's canvas is, so a phone offers that one first. */
export function shapeOfCanvas(width, height) {
  if (!(width > 0 && height > 0)) return "";
  if (width > height * 1.1) return "landscape";
  if (height > width * 1.1) return "portrait";
  return "square";
}

/**
 * The shape to start on. The one this person picked last time against a
 * mixer of the same shape wins; then the mixer's own shape, which its link
 * carries; then whatever they picked last; then landscape, which most
 * streams are. `remembered` is `{shape, link}`.
 */
export function startShape(remembered, fromLink) {
  const mine = remembered && isShape(remembered.shape) ? remembered.shape : "";
  if (mine && remembered.link === (fromLink || "")) return mine;
  if (isShape(fromLink)) return fromLink;
  return mine || "landscape";
}

export const quarter = (deg) => ((Math.round(deg / 90) * 90) % 360 + 360) % 360;

/**
 * How far the phone is turned from upright, from a `deviceorientation`
 * event's beta and gamma. Null when it is lying flat, where there is no up.
 * `last` is kept until the phone is clearly past the halfway point, so a
 * phone held at 45 degrees does not flick between two.
 */
export function deviceTurn(beta, gamma, last = null) {
  if (typeof beta !== "number" || typeof gamma !== "number") return null;
  const b = (beta * Math.PI) / 180;
  const g = (gamma * Math.PI) / 180;
  // Gravity across the screen: x towards the right edge, y towards the bottom.
  const x = Math.cos(b) * Math.sin(g);
  const y = Math.sin(b);
  if (Math.hypot(x, y) < 0.5) return last ?? null;
  const angle = ((Math.atan2(x, y) * 180) / Math.PI + 360) % 360;
  if (last !== null && last !== undefined) {
    const off = Math.abs(((angle - last + 540) % 360) - 180);
    if (off < 60) return last;
  }
  return quarter(angle);
}

/** How far the browser turned its screen, from `screen.orientation.angle` (anticlockwise). */
export function screenTurn(angle) {
  return quarter(-(Number(angle) || 0));
}

/**
 * The turn that puts the camera's picture upright. The browser already turns
 * the camera with the screen, so only what the screen did not follow is left.
 * A front camera's picture turns the other way round from a back camera's,
 * because it looks out of the other side. `manual` is the Rotate button's.
 */
export function correction({ device = null, screen = 0, facing = "", manual = 0, auto = true } = {}) {
  const left = auto && device !== null ? device - screen : 0;
  return quarter((facing === "user" ? -left : left) + manual);
}

/**
 * Where a picture `sw` x `sh`, turned by `turn`, lands on an `ow` x `oh`
 * canvas: the size to draw it at before turning, centred. `fill` covers the
 * canvas and crops; `fit` shows all of it with bars.
 */
export function placement(sw, sh, turn, ow, oh, fit = "fill") {
  if (!(sw > 0 && sh > 0)) return null;
  const sideways = turn === 90 || turn === 270;
  const [tw, th] = sideways ? [sh, sw] : [sw, sh];
  const scale = (fit === "fit" ? Math.min : Math.max)(ow / tw, oh / th);
  return { w: sw * scale, h: sh * scale };
}

/**
 * A sentence for a person holding the phone the other way from the shape they
 * chose, or "". A crop of an upright picture into a wide frame is mostly a
 * close up, which is worth knowing before going live.
 */
export function holdHint(shape, deviceTurnNow) {
  if (deviceTurnNow === null || deviceTurnNow === undefined) return "";
  const sideways = deviceTurnNow === 90 || deviceTurnNow === 270;
  if (shape === "landscape" && !sideways) return "Turn the phone on its side to get the whole wide picture.";
  if (shape === "portrait" && sideways) return "Hold the phone upright to get the whole tall picture.";
  return "";
}
