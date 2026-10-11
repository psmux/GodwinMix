// How a source looks on its tile: the icon and colour for its kind, and the
// kind read from its type or address. Out of kinds.js, which is the add
// source table and the plugin helpers, so the page every volunteer opens
// loads these few lines and not the rest of it.

export const ICONS = {
  camera: "M4 7h3l2-2h6l2 2h3v11H4V7zm8 3a3.5 3.5 0 100 7 3.5 3.5 0 000-7z",
  file: "M6 3h8l4 4v14H6V3zm8 1.5V8h3.5L14 4.5zM8 12h8v1.5H8V12zm0 3.5h8V17H8v-1.5z",
  page: "M3 5h18v14H3V5zm0 4h18M7 7V5m-4 2h18",
  stream: "M12 12a3 3 0 100 .01M6.5 6.5a8 8 0 000 11M17.5 6.5a8 8 0 010 11M3.5 3.5a12 12 0 000 17M20.5 3.5a12 12 0 010 17",
  exec: "M4 4h16v16H4V4zm3 4l4 4-4 4m6 1h5",
  graphic: "M4 4h16v12H4V4zm0 16h16M9 9l3 3 3-3",
  output: "M4 6h10v12H4V6zm12 3l5-3v12l-5-3V9z",
  media: "M5 4l14 8-14 8V4z",
  device: "M8 3h8v18H8V3zm3 16h2",
  screen: "M3 5h18v11H3V5zm6 15h6m-3-4v4",
  mic: "M12 3a3 3 0 013 3v5a3 3 0 01-6 0V6a3 3 0 013-3zM6 11a6 6 0 0012 0M12 17v4",
  pattern: "M4 4h16v16H4V4zm5.3 0v16m5.4-16v16M4 9.3h16M4 14.7h16",
  more: "M5 12h.01M12 12h.01M19 12h.01",
  text: "M5 5h14v3h-1.5V6.5h-4.75V17H15v1.5H9V17h2.25V6.5H6.5V8H5V5z",
};

export const KIND_COLOUR = {
  file: "var(--kind-file)",
  page: "var(--kind-page)",
  stream: "var(--kind-stream)",
  exec: "var(--kind-other)",
  camera: "var(--kind-camera)",
  graphic: "var(--kind-graphic)",
  screen: "var(--kind-camera)",
  mic: "var(--kind-graphic)",
  pattern: "var(--kind-other)",
  media: "var(--kind-file)",
  more: "var(--kind-other)",
  text: "var(--kind-graphic)",
};

/**
 * The kind a source's tile is drawn as. Its type says it plainly; a plugin
 * source's address is hidden ("…"), and reading the address alone drew every
 * camera, microphone and test pattern as a video file.
 */
const KIND_OF_TYPE = {
  camera: "camera", screen: "screen", "audio-device": "mic", test: "pattern", image: "media",
  browser: "page", layered: "page", ndi: "stream", srt: "stream", rtmp: "stream", hls: "stream",
  rtsp: "stream", rtp: "stream", udp: "stream", ingest: "stream", ipcam: "camera", decklink: "camera",
  icecast: "mic", file: "file", exec: "exec", text: "text", ticker: "text",
};

export function kindOfSource(source) {
  const prefix = String((source && (source.type || source.kind)) || "").split("/")[0];
  return KIND_OF_TYPE[prefix] || kindOfUri(source && source.uri);
}

/**
 * Work out a kind from a URI, the way the server does. Duplicated here on
 * purpose and only for the picker's first guess: the server decides, and
 * anything this gets wrong is corrected the moment the source reports back.
 */
export function kindOfUri(uri) {
  const u = String(uri || "").trim();
  if (!u) return "file";
  if (u.startsWith("exec:")) return "exec";
  if (u.startsWith("web+") || /^https?:\/\//i.test(u)) {
    if (/\.(m3u8|mpd)(\?|$)/i.test(u)) return "stream";
    return "page";
  }
  if (/^(rtmp|rtmps|srt|rtsp|udp|tcp|rist):\/\//i.test(u)) return "stream";
  if (/^file:\/\//i.test(u) || u.startsWith("/") || /^[a-z]:\\/i.test(u)) return "file";
  return "file";
}
