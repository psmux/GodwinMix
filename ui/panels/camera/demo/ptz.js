// What the demo answers for each ptz tool, in the plugin's own shapes
// (plugins/ptz/src/tools/), keyed as api.js's TOOLS table keys them so a
// renamed tool still reaches its handler here.

import * as view from "./view.js";
import { tune } from "./tune.js";

const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));

function listed(c, now) {
  return {
    id: c.id, name: c.name, protocol: "fake", address: "", user: null, profile: null, source: c.source, bus: null,
    capabilities: { absolute: true, position: true, focus: true, exposure: true, presets: true },
    moving: { pan: c.vel.pan, tilt: c.vel.tilt, zoom: c.vel.zoom, by_joystick: now - c.vel.at < 300 },
    last_error: null, tracking: c.tracking.on, added_by_tool: false,
  };
}

export function ptzTools(d) {
  const cam = (a) => {
    const c = a.camera ? d.cams.find((x) => x.id === a.camera) : d.cams.length === 1 ? d.cams[0] : null;
    if (!c) throw new Error(`there is no camera '${a.camera}'. camera_list names them: ${d.cams.map((x) => x.id).join(", ")}.`);
    return c;
  };
  const preset = (c, p, pictures) => {
    const out = { preset: p.preset, name: p.name, saved_at: p.saved_at, thumbnail: `presets/${c.id}/${p.preset}.jpg` };
    if (pictures) out.thumbnail_data = p.picture;
    return out;
  };
  return {
    cameras: () => ({ cameras: d.cams.map((c) => listed(c, d.now())), problems: [] }),
    move: (a) => {
      const c = cam(a);
      Object.assign(c.vel, { pan: +a.pan || 0, tilt: +a.tilt || 0, zoom: +a.zoom || 0, at: d.now(), zoomAt: d.now() });
      return { camera: c.id, pan: c.vel.pan, tilt: c.vel.tilt, zoom: c.vel.zoom, hold_ms: 300, last_error: null };
    },
    stop: (a) => {
      const c = cam(a);
      Object.assign(c.vel, { pan: 0, tilt: 0, zoom: 0, at: -1e9, zoomAt: -1e9 });
      Object.assign(c.tracking, { on: false, person: null });
      return { camera: c.id };
    },
    focus: (a) => {
      const c = cam(a);
      if (["auto", "manual"].includes(a.mode)) c.focus.mode = a.mode;
      else if (["near", "far"].includes(a.mode) && c.focus.mode === "auto") throw new Error(`'${c.id}' focus is automatic. Send focus {mode: "manual"} first, then near or far.`);
      return { camera: c.id, mode: a.mode };
    },
    exposure: (a) => {
      const c = cam(a);
      const e = Object.assign(c.exposure, ...["mode", "iris", "shutter", "gain"].filter((k) => a[k] !== undefined).map((k) => ({ [k]: a[k] })));
      e.brightness = e.mode === "auto" ? 0 : clamp((e.iris - 11) / 10 + e.gain / 20, -1, 1);
      return { camera: c.id, mode: a.mode ?? null, iris: e.iris, shutter: e.shutter, gain: e.gain };
    },
    position: (a) => {
      const v = cam(a).view;
      return { camera: cam(a).id, pan: (v.cx - 0.5) * 2, tilt: (0.5 - v.cy) * 2, zoom: 1 - v.w };
    },
    presets: (a) => ({ camera: cam(a).id, presets: cam(a).presets.map((p) => preset(cam(a), p, a.pictures)) }),
    presetSave: (a) => {
      const c = cam(a);
      const p = { preset: a.preset, name: a.name || `Preset ${a.preset}`, saved_at: Math.round(d.now()), picture: d.thumbnail(c), view: { ...c.view } };
      c.presets = [...c.presets.filter((x) => x.preset !== a.preset), p].sort((x, y) => x.preset - y.preset);
      return { camera: c.id, ...preset(c, p, false) };
    },
    presetRecall: (a) => {
      const c = cam(a);
      const p = c.presets.find((x) => x.preset === a.preset);
      if (!p) throw new Error(`'${c.id}' has no preset ${a.preset} saved through this plugin. preset_list shows the saved ones.`);
      Object.assign(c.tracking, { on: false, person: null });
      c.goal = { ...p.view };
      return { camera: c.id, preset: p.preset, name: p.name };
    },
    follow: (a) => {
      const c = cam(a);
      if (a.framing) c.tracking.style = a.framing;
      Object.assign(c.tracking, { on: true, person: view.choose(d.world, c, a), handUntil: 0 });
      return view.seen(d.world, c, d.now());
    },
    trackStop: (a) => {
      Object.assign(cam(a).tracking, { on: false, person: null });
      return { camera: cam(a).id, tracking: false };
    },
    framing: (a) => ((cam(a).tracking.style = a.framing), { camera: cam(a).id, framing: a.framing }),
    trackState: (a) => ({ ...view.state(d.world, cam(a), d.now()), tuning: tune(cam(a), {}).tuning }),
    tune: (a) => tune(cam(a), a),
    observe: (a) => ((cam(a).observedAt = d.now()), view.seen(d.world, cam(a), d.now())),
  };
}
