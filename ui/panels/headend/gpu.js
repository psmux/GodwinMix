// The GPU strip: per card, the encoder, the decoder, the 3D engine and the
// video memory as rings, each with two minutes of history under it, and
// the sessions, temperature, power and clocks beside them. From gpu.status;
// on a machine with no NVIDIA GPU, one line saying so and why.

import { el } from "../../shell/dom.js";
import { level, ring, spark } from "./charts.js";

const pct = (v) => (v == null ? "n/a" : `${v}%`);

function gauge(label, cls) {
  const r = ring(label, 64);
  const s = spark(cls, 64, 18);
  return { node: el("div.he-gauge", {}, [r.node, s.node]), r, s };
}

function card() {
  const g = { enc: gauge("encoder", "enc"), dec: gauge("decoder", "dec"), gpu: gauge("3D", "gfx"), mem: gauge("memory", "mem") };
  const name = el("span.he-gpuname");
  const facts = el("dl.he-facts");
  const node = el("div.he-gpu", {}, [el("div.he-gpuhead", {}, [name]), el("div.he-gauges", {}, [g.enc.node, g.dec.node, g.gpu.node, g.mem.node, facts])]);
  return { node, g, name, facts };
}

function fact(k, v, extra) {
  return [el("dt", { text: k }), el("dd", {}, [el("strong", { text: v }), extra || null])];
}

/** A thin bar for power against its limit. */
function power(s) {
  if (s.power_w == null) return null;
  const f = s.power_limit_w ? Math.min(1, s.power_w / s.power_limit_w) : 0;
  return el("span.he-pbar", { title: s.power_limit_w ? `limit ${s.power_limit_w} W` : "" }, [el("span", { style: { transform: `scaleX(${f})` } })]);
}

export function gpuStrip() {
  const cards = new Map();
  const note = el("p.he-gpunote");
  const list = el("div.he-gpus");
  const node = el("section.he-strip.he-gpustrip", { "aria-label": "GPU" }, [el("h3", { text: "GPU" }), list, note]);
  return {
    node,
    /** `w` is the MachineWatch. */
    draw(w) {
      const gpus = (w.gpu && w.gpu.gpus) || [];
      note.hidden = !!gpus.length;
      node.classList.toggle("none", !gpus.length);
      note.textContent = w.missing ? "This station has no gpu.status, so there is nothing to show here." : !w.gpu ? "Reading the GPU." : w.gpu.note || "No NVIDIA GPU on this machine.";
      for (const d of gpus) {
        if (!cards.has(d.index)) cards.set(d.index, card());
        const c = cards.get(d.index);
        if (c.node.parentNode !== list) list.append(c.node);
        draw(c, d);
      }
      for (const [k, c] of cards) if (!gpus.some((d) => d.index === k)) c.node.remove(), cards.delete(k);
    },
  };
}

function draw(c, d) {
  const s = d.now || {};
  const h = d.history || [];
  const series = (k) => h.map((x) => x[k] ?? 0);
  c.name.textContent = `${d.name}${d.driver ? `, driver ${d.driver}` : ""}`;
  c.g.enc.r.set(s.encoder_pct / 100, pct(s.encoder_pct), level(s.encoder_pct / 100));
  c.g.dec.r.set(s.decoder_pct / 100, pct(s.decoder_pct), level(s.decoder_pct / 100));
  c.g.gpu.r.set(s.gpu_pct / 100, pct(s.gpu_pct), level(s.gpu_pct / 100));
  const m = s.memory_total_mib ? s.memory_used_mib / s.memory_total_mib : null;
  c.g.mem.r.set(m, s.memory_total_mib ? `${(s.memory_used_mib / 1024).toFixed(1)}G` : "n/a", level(m));
  c.g.enc.s.set(series("encoder_pct"), 100);
  c.g.dec.s.set(series("decoder_pct"), 100);
  c.g.gpu.s.set(series("gpu_pct"), 100);
  c.g.mem.s.set(series("memory_used_mib"), s.memory_total_mib || undefined);
  const hot = s.temperature_c >= 83 ? ".hot" : s.temperature_c >= 75 ? ".busy" : "";
  c.facts.replaceChildren(
    ...fact("Sessions", s.encoder_sessions != null ? String(s.encoder_sessions) : "n/a", s.encoder_fps ? el("small", { text: ` at ${s.encoder_fps} fps` }) : null),
    ...fact("Temperature", s.temperature_c != null ? `${s.temperature_c} °C` : "n/a", hot ? el(`span.he-dot${hot}`) : null),
    ...fact("Power", s.power_w != null ? `${Math.round(s.power_w)} W` : "n/a", power(s)),
    ...fact("Clocks", s.graphics_mhz ? `${s.graphics_mhz} MHz` : "n/a", s.memory_mhz ? el("small", { text: ` mem ${s.memory_mhz}, video ${s.video_mhz || "n/a"}` }) : null),
    ...fact("Video memory", s.memory_total_mib ? `${s.memory_used_mib} of ${s.memory_total_mib} MiB` : "n/a"),
  );
}
