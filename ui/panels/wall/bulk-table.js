// The rows of a bulk add, each cell a box to fix before anything is sent,
// and the dry run's answer in words: the cost, whether it fits, and why a
// row was refused, under that row.

import { el } from "../../shell/dom.js";
import { FIELDS } from "./bulk-parse.js";

const HEAD = { name: "Name", input: "Input", program: "Program", output: "Output", format: "Format" };
const HINT = { name: "News", input: "udp://@239.1.1.1:5000", program: "first", output: "udp://10.0.0.50:6000", format: "copy" };

/**
 * @param {Array<object>} rows edited in place
 * @param {() => void} changed called on every edit, add and remove
 */
export function editTable(rows, changed, formats = []) {
  const list = el("datalist#wl-formats", {}, ["copy", ...formats].map((f) => el("option", { value: f })));
  const body = el("tbody");
  const table = el("table.wl-btable", {}, [
    el("thead", {}, el("tr", {}, [...FIELDS.map((f) => el("th", { text: HEAD[f], scope: "col" })), el("th", { "aria-label": "Remove" })])),
    body,
  ]);
  const draw = () => {
    body.replaceChildren(...rows.flatMap((r, i) => line(r, i)));
  };
  const line = (r, i) => {
    const cells = FIELDS.map((f) => el("td", { "data-label": HEAD[f] }, el("input.wl-bcell", {
      type: "text", value: r[f] || "", placeholder: HINT[f], "aria-label": `${HEAD[f]}, row ${i + 1}`, "data-field": f,
      list: f === "format" ? "wl-formats" : null, spellcheck: "false",
      oninput: (e) => { r[f] = e.target.value; changed(); },
    })));
    const del = el("button.btn.icon.wl-bdel", { type: "button", text: "×", title: "Remove this row", "aria-label": `Remove row ${i + 1}`, onclick: () => { rows.splice(i, 1); draw(); changed(); } });
    const tr = el(`tr.wl-brow${r.why ? ".refused" : ""}`, { "data-row": String(i) }, [...cells, el("td.wl-bdelcell", {}, del)]);
    if (!r.why) return [tr];
    return [tr, el("tr.wl-bwhy", {}, el("td", { colspan: String(FIELDS.length + 1), text: r.why }))];
  };
  draw();
  return { node: el("div.wl-bwrap", {}, [list, table]), draw };
}

const cores = (m) => (m < 1000 ? `${Math.round(m / 10)}% of a core` : `${(m / 1000).toFixed(1)} cores`);
const rate = (k) => (k >= 1000 ? `${(k / 1000).toFixed(k >= 10000 ? 0 : 1)} Mb/s` : `${Math.round(k)} kb/s`);

/** The dry run in sentences. */
export function planBox(answer, total) {
  const plan = answer.plan || {};
  const cost = plan.cost || {};
  const ok = (answer.added || []).length;
  const refused = (answer.refused || []).length;
  const parts = [];
  if (cost.cpu_millicores) parts.push(`${cores(cost.cpu_millicores)} of CPU for the encodes`);
  else parts.push("no encodes, every output is a copy");
  if (cost.device_millis) parts.push(`${Math.round(cost.device_millis / 10)}% of the GPU`);
  if (cost.egress_kbps) parts.push(`${rate(cost.egress_kbps)} of upload`);
  if (cost.memory_mib) parts.push(`${cost.memory_mib} MiB of memory`);
  const room = plan.room && plan.room.cpu_millicores;
  const fits = plan.fits !== false;
  const head = fits
    ? `${ok} of ${total} ${total === 1 ? "show is" : "shows are"} ready to add, and this machine has room for them.`
    : `This machine does not have room for every encode${room != null ? `: ${cores(room)} free` : ""}. The shows that fit are added and the rest are refused; set some formats to copy to add them all.`;
  return el(`div.wl-plan.${fits ? "fits" : "full"}`, { role: "status" }, [
    el("strong", { text: head }),
    el("span", { text: `It costs ${parts.join(", ")}.` }),
    refused ? el("span.wl-bad", { text: `${refused} ${refused === 1 ? "row is" : "rows are"} refused; the reason is under each one. Fix it and check again, or add the rest without them.` }) : null,
  ]);
}
