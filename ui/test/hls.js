// The script of /test/hls.html, a development page: hls.js against an HLS
// output on this core, with the numbers a person checks beside the picture.
// window.stats is what dev scripts read.
const q = new URLSearchParams(location.search);
const out = q.get("out") || "abr";
const src = `/hls/${encodeURIComponent(out)}/master.m3u8` + (q.get("key") ? `?key=${encodeURIComponent(q.get("key"))}` : "");
const video = document.getElementById("v");
const $ = (id) => document.getElementById(id);
const stats = { switches: [], errors: [], levels: [] };
window.stats = stats;
const log = (t) => { const d = document.createElement("div"); d.textContent = `${hms(Date.now())} ${t}`; $("log").prepend(d); };
function hms(ms) {
  const d = new Date(ms);
  const p = (n, w = 2) => String(n).padStart(w, "0");
  return `${p(d.getUTCHours())}:${p(d.getUTCMinutes())}:${p(d.getUTCSeconds())}.${p(d.getUTCMilliseconds(), 3)}`;
}
(function tick() { $("clock").textContent = hms(Date.now()); requestAnimationFrame(tick); })();

const hls = new Hls({ lowLatencyMode: q.get("ll") !== "0", backBufferLength: 30 });
window.hls = hls;
hls.loadSource(src);
hls.attachMedia(video);
hls.on(Hls.Events.MANIFEST_PARSED, (_, d) => {
  stats.levels = d.levels.map((l) => ({ height: l.height, bitrate: l.bitrate, codecs: l.codecSet }));
  $("levels").replaceChildren(...d.levels.map((l, i) => {
    const e = document.createElement("div");
    e.id = `lv${i}`;
    e.textContent = `${l.height}p  ${(l.bitrate / 1000).toFixed(0)} kbit/s  ${l.videoCodec || ""}`;
    return e;
  }));
  log(`manifest: ${d.levels.length} rungs`);
  video.play().catch(() => {});
});
hls.on(Hls.Events.LEVEL_SWITCHED, (_, d) => {
  const l = hls.levels[d.level];
  stats.switches.push({ at: Date.now(), height: l.height });
  document.querySelectorAll("#levels div").forEach((e) => e.classList.toggle("on", e.id === `lv${d.level}`));
  $("level").textContent = `${l.height}p, ${(l.bitrate / 1000).toFixed(0)} kbit/s`;
  log(`switched to ${l.height}p`);
});
hls.on(Hls.Events.ERROR, (_, d) => { stats.errors.push(`${d.type} ${d.details} ${d.fatal ? "fatal" : ""}`); log(`error ${d.details}${d.fatal ? " (fatal)" : ""}`); });
setInterval(() => {
  const b = video.buffered.length ? video.buffered.end(video.buffered.length - 1) - video.currentTime : 0;
  const pd = hls.playingDate ? (Date.now() - hls.playingDate.getTime()) / 1000 : null;
  Object.assign(stats, { latency: hls.latency, pdtLatency: pd, buffer: b, level: hls.currentLevel, playing: !video.paused && video.readyState > 2, bandwidth: hls.bandwidthEstimate });
  $("latency").textContent = hls.latency ? `${hls.latency.toFixed(2)} s` : "-";
  $("pdt").textContent = pd != null ? `${pd.toFixed(2)} s` : "-";
  $("buffer").textContent = `${b.toFixed(2)} s`;
}, 250);
