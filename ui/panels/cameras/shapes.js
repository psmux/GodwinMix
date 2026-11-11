// The ptz and atem plugins' answers, turned into what the camera window
// draws. The plugins' shapes are in plugins/ptz/src/tools/,
// plugins/ptz/src/follow/view.rs and plugins/atem/src/view.rs; the window's
// are small on purpose, so a second camera plugin only needs a second set of
// these.

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

/**
 * atem/state, as plugins/atem/src/view.rs answers it, in the window's terms:
 * one row per input with its long name, the M/E buses flat, the first M/E's
 * upstream keys and fade to black, and the macro to light.
 */
export function atem(s) {
  if (!s || !s.id) return s;
  const mes = s.mes || [];
  const first = mes[0] || {};
  const ftb = first.ftb || {};
  const run = s.macro || {};
  return {
    id: s.id,
    name: s.name || s.id,
    model: s.product || null,
    firmware: s.firmware || null,
    connected: !!s.connected,
    why: s.why || null,
    mode: s.mode || "own",
    inputs: (s.inputs || []).map((i) => ({ id: i.id, name: i.long || i.short || `Input ${i.id}`, short: i.short || "" })),
    map: s.map || {},
    mes: mes.map((m) => {
      const t = m.transition || {};
      return { program: m.program, preview: m.preview, style: t.style || "mix", rate: t.rate ?? null, position: t.position || 0, running: t.in_transition ? 1 : 0 };
    }),
    dsks: (s.dsk || []).map((d) => ({ on: !!d.on_air, tie: !!d.tie, rate: d.rate })),
    usks: (first.usk || []).map((k) => ({ on: !!k.on_air })),
    ftb: { on: !!ftb.on, rate: ftb.rate },
    auxes: (s.aux || []).map((a) => ({ source: a.input })),
    macros: (s.macros || []).map((m) => ({ index: m.index, name: m.name || `Macro ${m.index + 1}` })),
    lastMacro: run.running ? run.index : run.last_run ?? null,
    tally: s.tally || null,
  };
}
