// The demo's track_tune: the ptz plugin's numbers, their ranges and its
// refusal (plugins/ptz/schemas/tools/track_tune.json), kept on the camera.

const RANGES = {
  headroom: [0, 0.4, 0.1],
  lead_room: [0, 2, 0.5],
  dead_zone: [0, 0.3, 0.06],
  max_speed: [0.05, 1, 0.8],
  max_accel: [0.2, 10, 2.5],
  lost_hold_ms: [0, 30000, 2000],
  switch_margin: [1, 5, 1.5],
  detect_every: [1, 10, 1],
};

export function tune(cam, a) {
  cam.tune = cam.tune || { ...Object.fromEntries(Object.entries(RANGES).map(([k, r]) => [k, r[2]])), fallback: "stay" };
  const changed = [];
  for (const [k, [lo, hi]] of Object.entries(RANGES)) {
    if (a[k] === undefined) continue;
    if (!(a[k] >= lo && a[k] <= hi)) throw new Error(`${k} must be from ${lo} to ${hi}; ${a[k]} changes nothing.`);
  }
  if (a.fallback !== undefined && !/^(stay|wide|preset:\d+)$/.test(a.fallback)) throw new Error(`fallback is stay, wide or preset:<n>; '${a.fallback}' changes nothing.`);
  for (const k of [...Object.keys(RANGES), "fallback"]) if (a[k] !== undefined) (cam.tune[k] = a[k]), changed.push(k);
  return { camera: cam.id, tuning: { ...cam.tune }, changed };
}
