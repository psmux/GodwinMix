// The header band: the box's totals at a glance. Channels live of total,
// CPU and memory as rings, what comes in and goes out with two minutes of
// it, how long the machine and the station have been up, the Flussonic
// config the station follows when it follows one, and the buttons for
// importing a Flussonic config, full screen and close.

import { el, svg } from "../../shell/dom.js";
import { kbps } from "../wall/model.js";
import { level, ring, spark } from "./charts.js";

const gb = (mib) => (mib >= 10240 ? `${Math.round(mib / 1024)}` : `${(mib / 1024).toFixed(1)}`);

/** "3 d 4 h", "5 h 12 min", "8 min". */
export function upFor(secs) {
  if (secs == null) return "";
  const d = Math.floor(secs / 86400), h = Math.floor((secs % 86400) / 3600), m = Math.floor((secs % 3600) / 60);
  return d ? `${d} d ${h} h` : h ? `${h} h ${m} min` : `${m} min`;
}

export function band(act) {
  const chans = ring("live", 72);
  const cpu = ring("CPU", 72);
  const mem = ring("memory", 72);
  const inS = spark("in", 140, 30);
  const outS = spark("out", 140, 30);
  const v = () => el("strong.he-big");
  const [inV, outV, upV, upSub, host, problems] = [v(), v(), v(), el("span.he-small"), el("span.he-host"), el("span.he-problems")];
  const follow = el("span.he-host.he-follow", { hidden: true });
  const button = (key, icon, title) => el("button.btn.he-btn", { type: "button", "data-act": key, title, "aria-label": title }, [svg(icon, 18)]);
  const node = el("header.he-band", {}, [
    el("div.he-brand", {}, [el("h2", { text: "Headend" }), host, follow, problems]),
    el("div.he-cell.ring", {}, [chans.node]),
    el("div.he-cell.ring", {}, [cpu.node]),
    el("div.he-cell.ring", {}, [mem.node]),
    el("div.he-cell.net", {}, [el("span.he-k", { text: "Network in" }), inV, inS.node]),
    el("div.he-cell.net", {}, [el("span.he-k", { text: "Network out" }), outV, outS.node]),
    el("div.he-cell.up", {}, [el("span.he-k", { text: "Up" }), upV, upSub]),
    el("div.he-cell.acts", {}, [button("import", "M12 4v11M7 10l5 5 5-5M5 20h14", "Import a Flussonic config"), button("full", "M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5", "Full screen"), button("close", "M6 6l12 12M18 6L6 18", "Close the headend view (Escape)")]),
  ]);
  node.addEventListener("click", (e) => {
    const b = e.target.closest("[data-act]");
    if (b) act(b.dataset.act);
  });
  return {
    node,
    /** `t` is {live, total, problems, box, gov, inK, outK, inHist, outHist}. */
    draw(t) {
      chans.set(t.total ? t.live / t.total : 0, `${t.live}/${t.total}`, t.live < t.total ? "short" : "");
      const b = t.box || {};
      cpu.set(b.cpu_pct != null ? b.cpu_pct / 100 : null, b.cpu_pct != null ? `${Math.round(b.cpu_pct)}%` : "...", level(b.cpu_pct / 100));
      const used = b.memory_total_mib != null ? b.memory_total_mib - (b.memory_available_mib || 0) : null;
      mem.set(used != null ? used / b.memory_total_mib : null, used != null ? `${gb(used)}/${gb(b.memory_total_mib)}G` : "...", level(used / b.memory_total_mib));
      inV.textContent = kbps(t.inK) || "0 kb/s";
      outV.textContent = kbps(t.outK) || "0 kb/s";
      inS.set(t.inHist);
      outS.set(t.outHist);
      upV.textContent = upFor(b.uptime_secs) || "...";
      upSub.textContent = b.process_uptime_secs != null ? `station ${upFor(b.process_uptime_secs)}` : "";
      host.textContent = [b.host_name, b.cores ? `${b.cores} cores` : "", b.load_average ? `load ${b.load_average[0]}` : ""].filter(Boolean).join(" · ");
      const f = t.follow;
      follow.hidden = !(f && f.following);
      if (f && f.following) {
        follow.textContent = `Follows ${f.file}${f.listeners && f.listeners.length ? `, Flussonic URLs on ${f.listeners.join(", ")}` : ""}`;
        follow.title = `${f.shows} of ${f.streams} streams run as shows. ${f.note}`;
      }
      problems.textContent = t.problems ? `${t.problems} ${t.problems === 1 ? "channel needs" : "channels need"} a look` : "Every channel is live";
      problems.classList.toggle("bad", !!t.problems);
    },
  };
}
