// The script of /test/hls.html, a development page: hls.js (or dash.js with
// ?player=dash) against an HLS output on this core, with the numbers a
// person checks beside the picture. window.stats is what dev scripts read.
const q = new URLSearchParams(location.search);
const out = encodeURIComponent(q.get("out") || "abr");
const key = q.get("key") ? `?key=${encodeURIComponent(q.get("key"))}` : "";
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

/** Draw the ladder once the player knows it: `[{height, bitrate, codecs}]`. */
function ladder(levels) {
  stats.levels = levels;
  $("levels").replaceChildren(...levels.map((l, i) => {
    const e = document.createElement("div");
    e.id = `lv${i}`;
    e.textContent = `${l.height}p  ${(l.bitrate / 1000).toFixed(0)} kbit/s  ${l.codecs || ""}`;
    return e;
  }));
  log(`manifest: ${levels.length} rungs`);
}

function switched(i) {
  const l = stats.levels[i];
  if (!l) return;
  stats.switches.push({ at: Date.now(), height: l.height });
  document.querySelectorAll("#levels div").forEach((e) => e.classList.toggle("on", e.id === `lv${i}`));
  $("level").textContent = `${l.height}p, ${(l.bitrate / 1000).toFixed(0)} kbit/s`;
  log(`switched to ${l.height}p`);
}

function failed(text) {
  stats.errors.push(text);
  log(`error ${text}`);
}

/** Show what the player says about where it is, four times a second. */
function report(read) {
  setInterval(() => {
    const b = video.buffered.length ? video.buffered.end(video.buffered.length - 1) - video.currentTime : 0;
    const { latency, pdtLatency, level, bandwidth } = read();
    Object.assign(stats, { latency, pdtLatency, buffer: b, level, playing: !video.paused && video.readyState > 2, bandwidth });
    $("latency").textContent = latency ? `${latency.toFixed(2)} s` : "-";
    $("pdt").textContent = pdtLatency != null ? `${pdtLatency.toFixed(2)} s` : "-";
    $("buffer").textContent = `${b.toFixed(2)} s`;
  }, 250);
}

function playHls() {
  // No worker: the control port's CSP allows no blob: scripts, and hls.js
  // says so as an internalException before falling back to the main thread.
  const hls = new Hls({ lowLatencyMode: q.get("ll") !== "0", backBufferLength: 30, enableWorker: false });
  window.hls = hls;
  hls.loadSource(`/hls/${out}/master.m3u8${key}`);
  hls.attachMedia(video);
  hls.on(Hls.Events.MANIFEST_PARSED, (_, d) => {
    ladder(d.levels.map((l) => ({ height: l.height, bitrate: l.bitrate, codecs: l.videoCodec })));
    video.play().catch(() => {});
  });
  hls.on(Hls.Events.LEVEL_SWITCHED, (_, d) => switched(d.level));
  hls.on(Hls.Events.ERROR, (_, d) => failed(`${d.type} ${d.details}${d.fatal ? " fatal" : ""}${d.error && d.error.message ? `: ${d.error.message}` : ""}`));
  report(() => ({
    latency: hls.latency,
    pdtLatency: hls.playingDate ? (Date.now() - hls.playingDate.getTime()) / 1000 : null,
    level: hls.currentLevel,
    bandwidth: hls.bandwidthEstimate,
  }));
}

function playDash() {
  const player = dashjs.MediaPlayer().create();
  window.dash = player;
  player.initialize(video, `/hls/${out}/manifest.mpd${key}`, true);
  player.on(dashjs.MediaPlayer.events.STREAM_INITIALIZED, () => {
    ladder(player.getBitrateInfoListFor("video").map((b) => ({ height: b.height, bitrate: b.bitrate, codecs: "" })));
    switched(player.getQualityFor("video"));
  });
  player.on(dashjs.MediaPlayer.events.QUALITY_CHANGE_RENDERED, (e) => e.mediaType === "video" && switched(e.newQuality));
  player.on(dashjs.MediaPlayer.events.ERROR, (e) => failed(`dash ${e.error && (e.error.message || e.error.code)}`));
  report(() => ({ latency: player.getCurrentLiveLatency(), pdtLatency: null, level: player.getQualityFor("video"), bandwidth: null }));
}

if (q.get("player") === "dash") {
  const s = document.createElement("script");
  s.src = "vendor/dash.all.min.js";
  s.onload = playDash;
  document.body.append(s);
} else {
  playHls();
}
