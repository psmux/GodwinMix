// A mixed show's part of the map: its sources coming in, the scenes and the
// sound mixer that take them, the program with what is on air and in
// preview, the renditions made from it, and every output. Shapes are the
// status document (core.status), rendition.plan and output.list, as
// mixed.js reads them. A source's own rate is not measured, so its dots
// run at a steady pace that says only whether it is live and on air.

import { codec, healthOf } from "../wall/model.js";
import { toneOf } from "./tone.js";
import { made } from "./graph-direct.js";
import { outputFamily, outputKind, picture, rate, sourceKind, where } from "./words.js";

const PACE = { program: 2400, preview: 1200, off: 500 };

function showTone(show) {
  if (show.state === "stopped") return "off";
  if (show.state === "failed") return "bad";
  if (show.state === "starting") return "busy";
  const h = healthOf(show).state;
  return h === "alarm" ? "bad" : h === "warning" ? "warn" : "ok";
}

const onAir = (d) => {
  const src = d.program && (d.sources || []).find((s) => s.id === d.program);
  return d.sceneName || d.scene || (src && src.name) || d.program || "";
};

export function addMixed(g, show, d) {
  const id = `show:${show.id}`;
  const prog = `prog:${show.id}`;
  const tone = showTone(show);
  const srcs = (d && d.sources) || [];
  const tally = (d && d.tally) || {};
  const live = show.state === "running" || show.state === "live";
  const air = d ? onAir(d) : "";
  const rends = renditions(g, show, d, id);
  g.node({ id, col: 1, kind: "show", show: show.id, mixed: true, tone, state: show.state, title: show.name || "This mixer", num: "", onAir: air, preview: d && d.previewName,
    lines: [`Scenes and sound · ${srcs.length} ${srcs.length === 1 ? "source" : "sources"}`, !d ? (live ? "Reading" : "Stopped") : air ? `On air: ${air}` : live ? "On air: black" : "Stopped", d && d.previewName ? `Preview: ${d.previewName}` : ""], rate: show.programme_kbps || 0 });
  for (const s of srcs) {
    const sid = `src:${show.id}:${s.id}`;
    const t = tally[s.id] || (d.program === s.id ? "program" : d.preview === s.id ? "preview" : "off");
    const stone = toneOf(s.state === "live" ? "live" : s.state);
    g.node({ id: sid, col: 0, kind: "source", show: show.id, source: s, tally: t, tone: stone, title: s.name || s.id,
      lines: [sourceKind(s), t === "program" ? "On air" : t === "preview" ? "Preview" : [s.has_video && "picture", s.has_audio && "sound"].filter(Boolean).join(" and ")] });
    g.link(`${sid}>${id}`, sid, id, s.state === "live" ? PACE[t] || PACE.off : 0, stone, { nominal: true, onair: t === "program" });
  }
  // One encode of the program: it is the program node. Several: each is a
  // rendition, and what is on air is said on the show's own card.
  const b = (d && d.backend) || {};
  if (!rends.size) {
    g.node({ id: prog, col: 2, kind: "program", show: show.id, tone: live ? (d && !air ? "warn" : "ok") : "off", title: "Program",
      lines: [air ? `On air: ${air}` : live ? "Black" : "Stopped", b.video_encoder ? `${b.hardware_accelerated ? "GPU" : "CPU"} encode` : ""], onAir: air, preview: d && d.previewName, backend: b, rate: show.programme_kbps || 0 });
    g.link(`${id}>${prog}`, id, prog, live ? show.programme_kbps || PACE.program : 0, live ? "ok" : "off", { nominal: !show.programme_kbps });
  }
  for (const o of (d && d.outputs) || []) {
    const oid = `out:${show.id}:${o.id}`;
    const kbps = (d.rates && d.rates.get(o.id)) || 0;
    const state = o.state || "connecting";
    const otone = toneOf(state);
    g.node({ id: oid, col: 3, kind: "output", show: show.id, output: o, tone: otone, state, title: outputKind(o), family: outputFamily(o), rate: kbps,
      lines: [o.error ? String(o.error.message || o.error.code || o.error) : where(o), [rate(kbps), o.reconnects ? `${o.reconnects} reconnects` : ""].filter(Boolean).join(" · ")] });
    const up = rends.get(o.id) || (rends.size ? id : prog);
    g.link(`${up}>${oid}`, up, oid, kbps || (state === "live" ? PACE.program : 0), otone, { nominal: !kbps });
  }
  const mv = d && d.multiview;
  if (mv && mv.enabled) {
    const oid = `out:${show.id}:multiview`;
    g.node({ id: oid, col: 3, kind: "output", show: show.id, output: { id: "multiview", kind: "multiview" }, tone: "ok", state: "live", title: "Multiview", family: "multiview",
      lines: [`${mv.cols} by ${mv.rows}, ${picture(mv.width, mv.height, mv.fps)}`, ""] });
    g.link(`${id}>${oid}`, id, oid, PACE.preview, "ok", { nominal: true });
  }
}

/** "720p H.264" and its rate from a plan node's id: `encode:programme:h264:1280x720p30:2800k`. */
function shapeOf(id) {
  const size = /:(\d+)x(\d+)(?:p(\d+))?/.exec(id), k = /:(\d+)k(?::|$)/.exec(id), c = /^encode:[^:]+:([a-z0-9]+)/.exec(id);
  return { title: [size && picture(+size[1], +size[2], size[3] && +size[3]), c && codec(c[1])].filter(Boolean).join(" ") || "Program encode", kbps: k ? +k[1] : 0 };
}

/** Each encode the plan makes from the program, and which outputs it serves. */
function renditions(g, show, d, from) {
  const serves = new Map();
  const nodes = ((d && d.plan && d.plan.nodes) || []).filter((n) => n.kind === "encode" && n.serves && n.serves.length);
  if (nodes.length < 2) return serves;
  for (const n of nodes) {
    const rid = `rend:${show.id}:${n.id}`;
    const e = n.encoder || {};
    const hw = typeof e === "object" ? !!e.hardware : /nv|vaapi|qsv|videotoolbox|amf/i.test(String(e));
    const o = (d.outputs || []).find((x) => n.serves.includes(x.id)) || {};
    const m = o.rendition ? made(o) : shapeOf(n.id);
    g.node({ id: rid, col: 2, kind: "proc", show: show.id, tone: n.shed ? "warn" : "ok", gpu: hw, title: m.title, makes: [m.title, rate(m.kbps)].filter(Boolean).join(" "),
      lines: [hw ? "GPU" : "CPU", [rate(m.kbps), `for ${n.serves.length} ${n.serves.length === 1 ? "output" : "outputs"}`].filter(Boolean).join(" · ")], encoder: e.id || String(e), reason: n.reason && n.reason.text, shed: n.shed, serves: n.serves, steps: [] });
    g.link(`${from}>${rid}`, from, rid, show.programme_kbps || PACE.program, "ok", { nominal: !show.programme_kbps });
    for (const s of n.serves) serves.set(s, rid);
  }
  return serves;
}
