// What the demo answers for each tool in api.js's TOOLS table, keyed the
// same way: the ptz plugin's tools in ptz.js, the switchers' here.

import * as atem from "./atem.js";
import { ptzTools } from "./ptz.js";
import { camera } from "./rig.js";

export function handlers(d) {
  const sw = (a) => {
    const s = d.atems.find((x) => x.id === a.switcher);
    if (!s) throw new Error(`no switcher called '${a.switcher}'; atem/list names them`);
    return s;
  };
  const ok = () => ({ ok: true });
  return {
    ...ptzTools(d),
    atemList: () => ({ switchers: d.atems.map((s) => ({ id: s.id, name: s.name, product: s.model, connected: s.connected, mode: s.mode, why: s.connected ? null : "the demo switcher is off" })) }),
    atemState: (a) => atem.toPlugin(sw(a)),
    cut: (a) => (atem.cut(sw(a), a.me || 0), ok()),
    auto: (a) => (atem.auto(sw(a), a.me || 0), ok()),
    preview: (a) => (atem.setBus(sw(a), "preview", a.input, a.me || 0), ok()),
    program: (a) => (atem.setBus(sw(a), "program", a.input, a.me || 0), ok()),
    dsk: (a) => (atem.dsk(sw(a), a.keyer, a.action), ok()),
    usk: (a) => ((sw(a).usks[a.keyer].on = !!a.on), ok()),
    ftb: (a) => ((sw(a).ftb.on = !sw(a).ftb.on), ok()),
    aux: (a) => ((sw(a).auxes[a.aux].source = a.input), ok()),
    macro: (a) => ((sw(a).lastMacro = a.index), ok()),
    transition: (a) => (Object.assign(sw(a).mes[a.me || 0], a.style ? { style: a.style } : {}, a.rate ? { rate: a.rate } : {}), ok()),
    mode: (a) => ((sw(a).mode = a.mode), ok()),
    add: (a) => {
      d.cams = d.cams.filter((c) => c.id !== a.id).concat(camera(a.id, a.id, { cx: 0.5, cy: 0.62, w: 0.5 }));
      d.cams[d.cams.length - 1].source = a.source || a.id;
      return { camera: a.id };
    },
    connect: (a) => {
      const s = atem.switcher(a.id, a.name || a.id, "ATEM Mini", 4, d.cams, a.mode || "own");
      d.atems = d.atems.filter((x) => x.id !== a.id).concat(s);
      return { switcher: a.id, connected: true };
    },
    disconnect: (a) => ((d.atems = d.atems.filter((x) => x !== sw(a))), ok()),
    map: (a) => {
      const s = sw(a);
      if (a.input === null) delete s.map[a.source];
      else s.map[a.source] = a.input;
      return { switcher: s.id, map: s.map };
    },
  };
}
