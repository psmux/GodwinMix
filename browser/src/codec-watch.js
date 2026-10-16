// Injected into every page the sidecar renders whole. Says on the console
// when the page could not play a video because this browser has no decoder
// for it, so the mixer can tell the operator why the picture shows an error
// rather than leave them guessing.
//
// The official CEF build carries no H.264 and no AAC. Most sites send VP9,
// AV1 and Opus to a browser that cannot take those, so nothing shows; but a
// YouTube live stream often has its sound only in AAC, and a site that only
// has H.264 files fails outright. GodwinMix's own build has both, and then
// the format most often missing is HEVC, which Chromium decodes only on the
// GPU: with the GPU off (`params.gpu`, off by default) it is not offered at
// all. Two signs: a media element that fails with MEDIA_ERR_SRC_NOT_SUPPORTED,
// and YouTube's own player error. The sidecar sets `window.__gmxGpu` first.
(() => {
  if (window.__gmxCodecWatch) return;
  window.__gmxCodecWatch = true;
  const TAG = "GMX_NOTICE ";
  let said = "";
  const say = (why) => {
    if (why === said) return;
    said = why;
    console.log(TAG + why);
  };
  const can = (t) => typeof MediaSource !== "undefined" && MediaSource.isTypeSupported(t);
  const missing = () => !can('video/mp4; codecs="avc1.4d401f"') || !can('audio/mp4; codecs="mp4a.40.2"');
  const hevc = () => can('video/mp4; codecs="hvc1.1.6.L93.B0"');
  const failed = (el) => {
    if (missing()) {
      say("codecs: a video on this page is in a format this browser cannot play (H.264 or AAC)");
    } else if (!hevc() && !window.__gmxGpu) {
      say("codecs: a video on this page is in a format this browser cannot play. If it is HEVC (H.265), " +
          "it plays with hardware video decoding, which is off for this source: set params.gpu to true " +
          "(Hardware video decoding in the source's settings)");
    } else if (!hevc()) {
      say("codecs: a video on this page is in a format this browser cannot play. HEVC (H.265) is one: " +
          "params.gpu is on, but this machine's GPU does not decode it");
    } else {
      say("codecs: a video on this page is in a format this browser cannot play");
    }
  };
  const unplayable = (el) => el && (el.tagName === "VIDEO" || el.tagName === "AUDIO") && el.error && el.error.code === 4;
  document.addEventListener("error", (e) => { if (unplayable(e.target)) failed(e.target); }, true);
  // A video that failed before this script arrived, which is injected when
  // the page has finished loading.
  document.querySelectorAll("video, audio").forEach((el) => { if (unplayable(el)) failed(el); });
  const yt = () => {
    const err = document.querySelector(".ytp-error");
    if (err && err.offsetParent !== null && missing()) {
      say("codecs: YouTube says this browser cannot play the video; it is offered only in H.264 or AAC");
    }
  };
  if (/(^|\.)youtube(-nocookie)?\.com$/.test(location.hostname)) {
    const timer = setInterval(yt, 2000);
    addEventListener("pagehide", () => clearInterval(timer));
  }
})();
