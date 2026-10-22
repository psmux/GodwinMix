// A player for a channel's HLS, in the detail drawer. The link is the HLS
// output's own `playback.master_url_path` from show.list, so it carries the
// viewer key and plays what a viewer gets. Safari and the macOS app play it
// in a plain <video>; elsewhere hls.js, shipped beside the page in
// ui/vendor/ and fetched the first time a drawer opens a player, never from
// a CDN. Its worker is off: the page's policy allows workers only from
// this origin, and one channel at a time needs none.

import { el } from "../../shell/dom.js";

const HLSJS = new URL("../../vendor/hls.light.min.js", import.meta.url).href;
let loading = null;

/** hls.js, loaded once, by a <script> since it is not a module. */
function hlsjs() {
  if (window.Hls) return Promise.resolve(window.Hls);
  loading ||= new Promise((resolve, reject) => {
    const s = el("script", { src: HLSJS });
    s.onload = () => resolve(window.Hls);
    s.onerror = () => { loading = null; reject(new Error("hls.js could not be loaded")); };
    document.head.append(s);
  });
  return loading;
}

/** The first enabled HLS output with a link, or null. */
export function hlsOutput(show) {
  return (show.outputs || []).find((o) => o.enabled !== false && o.playback && o.playback.master_url_path) || null;
}

const native = () => {
  const v = document.createElement("video");
  return !!(v.canPlayType && v.canPlayType("application/vnd.apple.mpegurl"));
};

export function player(client, show) {
  const out = hlsOutput(show);
  const video = el("video.he-video", { muted: true, autoplay: true, playsInline: true, controls: true });
  const say = el("p.he-playsay");
  const node = el("div.he-player", {}, [video, say]);
  let hls = null;
  let stopped = false;
  if (!out) {
    say.textContent = "This channel has no HLS output, so there is nothing here to play. Add one with the address hls:// and a name.";
    video.hidden = true;
    return { node, stop() {} };
  }
  const base = (client.transport && client.transport.base) || location.origin;
  const url = new URL(out.playback.master_url_path, base).href;
  say.textContent = out.state && out.state !== "live" && out.error ? `The output ${out.id} is ${out.state}: ${out.error}` : `HLS from the output ${out.id}.`;
  if (native()) {
    video.src = url;
  } else {
    hlsjs().then((Hls) => {
      if (stopped) return;
      if (!Hls.isSupported()) return void (say.textContent = "This browser cannot play HLS.");
      hls = new Hls({ enableWorker: false, liveSyncDurationCount: 3 });
      hls.on(Hls.Events.ERROR, (_, d) => { if (d.fatal) say.textContent = `The player stopped: ${d.details}.`; });
      hls.on(Hls.Events.MANIFEST_PARSED, (_, d) => { say.textContent = `HLS from the output ${out.id}: ${d.levels.length} ${d.levels.length === 1 ? "rendition" : "renditions"}, ${hls.audioTracks.length || 1} sound ${hls.audioTracks.length > 1 ? "tracks" : "track"}.`; });
      hls.loadSource(url);
      hls.attachMedia(video);
    }).catch((e) => { say.textContent = e.message; });
  }
  return {
    node,
    stop() {
      stopped = true;
      if (hls) hls.destroy();
      video.removeAttribute("src");
      video.load();
    },
  };
}
