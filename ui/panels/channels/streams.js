// One stream on a channel: what the encoder is sending, who it is, how long
// for, and which mixer source it feeds. The bitrate is a sparkline of the
// last forty readings, kept by the model, which the panel takes every two
// seconds while the stream is live.

import { el, svg } from "../../shell/dom.js";
import { write } from "./keyed.js";
import { badges, resolution, fmtFps, fmtKbps, fmtUptime, keyLabel, startedAt, streamKbps } from "./model.js";

const CLOCK = "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 7v5l3 2";
const NS = "http://www.w3.org/2000/svg";
const W = 96;
const H = 26;

export function streamRow(view, getChannel) {
  const dot = el("span.chn-sdot");
  const name = el("strong.chn-sname");
  const who = el("span.chn-who");
  const res = el("span.chn-spec");
  const fps = el("span.chn-spec");
  const chips = el("span.chn-badges");
  const spark = sparkline();
  const rate = el("span.chn-rate");
  const since = el("span");
  const clock = el("span.chn-uptime", { title: "Publishing for" }, [svg(CLOCK, 13), since]);
  const feeds = el("span.chn-feeds");
  const node = el("div.chn-stream", {}, [
    dot,
    el("div.chn-sid", {}, [name, who]),
    el("div.chn-specs", {}, [res, fps, chips]),
    el("div.chn-bits", {}, [spark.node, rate]),
    clock,
    feeds,
  ]);
  let began = 0;
  let current = null;

  function update(s) {
    current = s;
    const channel = getChannel();
    const live = s.state === "live";
    node.classList.toggle("idle", !live);
    write(dot, "className", "chn-sdot" + (live ? " live" : ""));
    write(name, "textContent", s.name);
    const label = keyLabel(channel, s.key);
    write(who, "textContent", live ? [s.from && `from ${s.from}`, label].filter(Boolean).join(", ") : "Not publishing");
    write(res, "textContent", resolution(s));
    res.hidden = !res.textContent;
    write(fps, "textContent", s.video && s.video.fps ? fmtFps(s.video.fps) : "");
    fps.hidden = !fps.textContent;
    const words = badges(s).join("|");
    if (chips.dataset.words !== words) {
      chips.dataset.words = words;
      chips.replaceChildren(...badges(s).map((b) => el("span.chn-badge", { text: b })));
    }
    spark.draw(view.model.samples(channel.id, s.name));
    write(rate, "textContent", live ? fmtKbps(streamKbps(s)) : "");
    began = live ? startedAt(s.since_ms, view.model.seenAt.get(channel.id + "/" + s.name)) : 0;
    tick();
    write(feeds, "textContent", s.source ? s.source : channel.auto_source ? (live ? "Becoming a source" : "") : "Not a source");
    feeds.classList.toggle("on", !!s.source);
    feeds.title = s.source ? `Feeds the mixer source ${s.source}` : "";
  }

  function tick() {
    write(since, "textContent", began ? fmtUptime(Date.now() - began) : "");
    clock.classList.toggle("none", !began);
  }

  return { node, update, tick, get stream() { return current; } };
}

/** A line and a soft fill under it, drawn from a list of numbers. */
export function sparkline() {
  const svg = document.createElementNS(NS, "svg");
  svg.setAttribute("viewBox", `0 0 ${W} ${H}`);
  svg.setAttribute("class", "chn-spark");
  svg.setAttribute("aria-hidden", "true");
  const area = document.createElementNS(NS, "path");
  area.setAttribute("class", "chn-spark-area");
  const line = document.createElementNS(NS, "path");
  line.setAttribute("class", "chn-spark-line");
  svg.append(area, line);
  return {
    node: svg,
    draw(samples) {
      const d = sparkPath(samples);
      line.setAttribute("d", d.line);
      area.setAttribute("d", d.area);
    },
  };
}

/** The path for a sparkline, scaled so the busiest sample nearly fills it. */
export function sparkPath(samples) {
  if (!samples.length) return { line: "", area: "" };
  const list = samples.length === 1 ? [samples[0], samples[0]] : samples;
  const top = Math.max(...list) * 1.15 || 1;
  const step = W / (list.length - 1);
  const pts = list.map((v, i) => `${(i * step).toFixed(1)} ${(H - 2 - (v / top) * (H - 4)).toFixed(1)}`);
  const line = "M" + pts.join("L");
  return { line, area: `${line}L${W} ${H}L0 ${H}Z` };
}
