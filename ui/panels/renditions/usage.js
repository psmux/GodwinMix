// Who is using what, from a plan and a governor status. Pure.

import { encoderOf } from "./words.js";

const KIND_WORDS = { decode: "Decoding", scale: "Scaling", convert: "Converting", mux: "Packaging", copy: "Copying", package: "Packaging" };

function label(node) {
  const enc = encoderOf(node);
  const what = enc ? enc.id : KIND_WORDS[node.kind] || node.kind || "Work";
  const who = (node.serves || []).join(", ");
  return who ? `${what} for ${who}` : what;
}

function merge(rows) {
  const by = new Map();
  for (const r of rows) by.set(r.label, (by.get(r.label) || 0) + r.value);
  return [...by].map(([l, value]) => ({ label: l, value })).sort((a, b) => b.value - a.value);
}

/**
 * What is using the CPU, in millicores, largest first. What the plan does
 * not account for is the mixer's own work: compositing, previews, the page.
 */
export function cpuUsers(plan, status) {
  const rows = [];
  for (const n of (plan && plan.nodes) || []) {
    const cpu = (n.cost && n.cost.cpu_millicores) || 0;
    if (cpu > 0) rows.push({ label: label(n), value: cpu });
  }
  const merged = merge(rows);
  const used = (status && status.cpu && status.cpu.used_millicores) || 0;
  const rest = used - merged.reduce((s, r) => s + r.value, 0);
  if (rest > 20) merged.push({ label: "The mixer itself", value: rest, title: "The programme, the previews and this page" });
  return merged;
}

/** What is using one GPU, in thousandths of it. */
export function deviceUsers(plan, device, deviceCount) {
  const rows = [];
  for (const n of (plan && plan.nodes) || []) {
    const enc = encoderOf(n);
    const millis = (n.cost && n.cost.device_millis) || 0;
    if (!enc || !enc.hardware || !millis) continue;
    if (enc.device && enc.device !== device.id && deviceCount > 1) continue;
    rows.push({ label: label(n), value: millis });
  }
  const merged = merge(rows);
  const rest = (device.used_millis || 0) - merged.reduce((s, r) => s + r.value, 0);
  if (rest > 20) merged.push({ label: "Other work on this device", value: rest });
  return merged;
}

/** True while anything goes out: a destination live or connecting, or a recording. */
export function onAir(state) {
  return ((state && state.outputs) || []).some((o) => o.state === "live" || o.state === "connecting" || o.state === "reconnecting");
}

/** A device's name for a heading: "GPU (apple-m1)". */
export function deviceTitle(d) {
  const kind = { gpu: "GPU", npu: "Media engine", vpu: "Media engine" }[String(d.kind || "gpu").toLowerCase()] || String(d.kind).toUpperCase();
  return d.id ? `${kind} (${d.id})` : kind;
}

/** Thousandths of a device as a percentage. */
export const share = (m) => `${Math.round((m || 0) / 10)}%`;

/** Millicores in the unit a heading uses: "3.2 cores". */
export const coresShort = (m) => `${Math.round((m || 0) / 100) / 10} cores`;
