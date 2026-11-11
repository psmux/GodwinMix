// What the demo answers for each tool in api.js's TOOLS table, keyed the
// same way, so a renamed tool still reaches its handler here.

import { observe } from "./world.js";
import { trackState, aimAt } from "./rig.js";
import * as atem from "./atem.js";

const pub = (c) => ({ id: c.id, name: c.name, source: c.source, model: c.model, connected: c.connected });

export function handlers(d) {
  const cam = (a) => {
    const c = d.cams.find((x) => x.id === a.camera);
    if (!c) throw new Error(`no camera called '${a.camera}'; ptz/camera_list names them`);
    return c;
  };
  const sw = (a) => {
    const s = d.atems.find((x) => x.id === a.switcher);
    if (!s) throw new Error(`no switcher called '${a.switcher}'; atem/list names them`);
    return s;
  };
  const ok = (x) => x || { ok: true };
  const following = (c, person) => Object.assign(c.tracking, { on: true, person, point: null });
  return {
    cameras: () => ({ cameras: d.cams.map(pub) }),
    move: (a) => ok(Object.assign(cam(a).vel, { pan: +a.pan || 0, tilt: +a.tilt || 0, at: d.now() }) && null),
    stop: (a) => ok(Object.assign(cam(a).vel, { pan: 0, tilt: 0, zoom: 0, at: -1e9, zoomAt: -1e9 }) && null),
    zoom: (a) => ok(Object.assign(cam(a).vel, { zoom: +a.speed || 0, zoomAt: a.speed ? d.now() : -1e9 }) && null),
    focus: (a) => ({ focus: Object.assign(cam(a).focus, a.auto !== undefined ? { auto: !!a.auto } : { auto: false, near: Math.max(0, Math.min(1, cam(a).focus.near + (a.speed || 0) * 0.05)) }) }),
    exposure: (a) => ({ exposure: Object.assign(cam(a).exposure, a.mode ? { mode: a.mode } : {}, a.brightness !== undefined ? { brightness: a.brightness } : {}) }),
    presets: (a) => ({ presets: cam(a).presets.map(({ view, ...p }) => p) }),
    presetSave: (a) => {
      const c = cam(a);
      const p = { slot: a.slot, name: a.name || `Shot ${a.slot + 1}`, thumbnail: d.thumbnail(c), view: { ...c.view } };
      c.presets = [...c.presets.filter((x) => x.slot !== a.slot), p].sort((x, y) => x.slot - y.slot);
      return { slot: p.slot, name: p.name, thumbnail: p.thumbnail };
    },
    presetRecall: (a) => {
      const c = cam(a);
      const p = c.presets.find((x) => x.slot === a.slot);
      if (!p) throw new Error(`camera '${c.id}' has no preset ${a.slot}; ptz/preset_list shows the saved ones`);
      c.tracking.person = null;
      c.goal = { ...p.view };
      return ok();
    },
    track: (a) => {
      const c = cam(a);
      c.tracking.on = !!a.on;
      if (a.on && !c.tracking.person) c.tracking.person = (observe(d.world, c)[0] || {}).id || null;
      return trackState(c, d.now());
    },
    framing: (a) => ok((cam(a).tracking.style = a.style) && null),
    observe: (a) => ({ camera: a.camera, width: 1280, height: 720, people: observe(d.world, cam(a)), following: cam(a).tracking.person }),
    follow: (a) => {
      const c = cam(a);
      if (a.person) following(c, a.person);
      else if (a.point) Object.assign(c.tracking, { on: true, person: null, point: a.point }), aimAt(c, a.point[0], a.point[1]);
      c.tracking.handUntil = 0;
      return trackState(c, d.now());
    },
    trackStop: (a) => ok(Object.assign(cam(a).tracking, { person: null, point: null }) && null),
    trackState: (a) => trackState(cam(a), d.now()),
    atemList: () => ({ switchers: d.atems.map((s) => ({ id: s.id, name: s.name, model: s.model, connected: s.connected, mode: s.mode })) }),
    atemState: (a) => atem.snapshot(sw(a)),
    cut: (a) => ok(atem.cut(sw(a), a.me || 0)),
    auto: (a) => ok(atem.auto(sw(a), a.me || 0)),
    preview: (a) => ok(atem.setBus(sw(a), "preview", a.input, a.me || 0)),
    program: (a) => ok(atem.setBus(sw(a), "program", a.input, a.me || 0)),
    dsk: (a) => ok(atem.dsk(sw(a), a.keyer, a.action)),
    usk: (a) => ok((sw(a).usks[a.keyer].on = !!a.on) && null),
    ftb: (a) => ok((sw(a).ftb.on = !sw(a).ftb.on) && null),
    aux: (a) => ok((sw(a).auxes[a.aux].source = a.input) && null),
    macro: (a) => ok((sw(a).lastMacro = a.index) && null),
    transition: (a) => ok(Object.assign(sw(a).mes[a.me || 0], a.style ? { style: a.style } : {}, a.rate ? { rate: a.rate } : {}) && null),
    mode: (a) => ok((sw(a).mode = a.mode) && null),
  };
}
