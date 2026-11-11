// The ptz plugin's answers, turned into what the camera window draws. The
// plugin's shapes are in plugins/ptz/src/tools/ and plugins/ptz/src/follow/
// view.rs; the window's are small on purpose, so a second camera plugin
// only needs a second set of these.

/** "cam-pulpit" reads "Pulpit"; a name the plugin gives wins. */
export function nameOf(id) {
  const words = String(id).replace(/^cam-/, "").split("-").filter(Boolean);
  return words.length ? words.map((w, i) => (i ? w : w[0].toUpperCase() + w.slice(1))).join(" ") : String(id);
}

/** One camera from camera_list. */
export function camera(c) {
  return {
    id: c.id,
    name: c.name || nameOf(c.id),
    source: c.source || c.id,
    protocol: c.protocol,
    connected: !c.last_error,
    error: c.last_error || null,
    tracking: !!c.tracking,
    can: c.capabilities || {},
  };
}

/** One preset from preset_list {pictures: true}. */
export function preset(p) {
  return { slot: p.preset, name: p.name || `Shot ${p.preset}`, thumbnail: p.thumbnail_data || null };
}

/** The people a track answer lists, with boxes as [x, y, w, h]. */
export function people(s) {
  const tracks = (s && s.tracks) || [];
  const list = tracks.map((t) => ({ id: t.id, box: [t.box.x, t.box.y, t.box.w, t.box.h], score: t.confidence, followed: !!t.followed }));
  const followed = list.find((t) => t.followed);
  return {
    camera: s && s.camera,
    people: list,
    following: followed ? followed.id : s && s.mode === "following" ? s.target || null : null,
    error: (s && s.error) || null,
  };
}

/**
 * track_state as the window says it. The plugin's `mode` is stopped,
 * observing or following; its `state` is idle, following, holding or a
 * fallback, which the window says as following, looking for someone, or
 * lost and holding the shot.
 */
export function tracking(s) {
  if (!s) return null;
  const on = s.mode === "following";
  let state = "off";
  if (s.error) state = "error";
  else if (on && s.state === "following") state = "following";
  else if (on && (s.state === "holding" || String(s.state).startsWith("fallback"))) state = "lost";
  else if (on) state = "searching";
  return { camera: s.camera, on, state, person: s.target || null, style: s.framing || null, error: s.error || null, fps: s.fps };
}
