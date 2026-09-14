// Addresses, built the same way in all three client libraries.
//
// A token goes in the query rather than a header because an <img> tag, a
// WebSocket and a WHEP player cannot set headers. GET only: a token in the URL
// of a POST ends up in more logs than it should.

function base(url: string): URL {
  return new URL(url.endsWith("/") ? url : url + "/");
}

/** `ws://host/rpc?token=`, from an http, https, ws or wss address. */
export function rpcUrl(origin: string, token?: string | null): string {
  const u = new URL("/rpc", base(origin));
  u.protocol = u.protocol === "https:" || u.protocol === "wss:" ? "wss:" : "ws:";
  if (token) u.searchParams.set("token", token);
  return u.toString();
}

/**
 * `GET /api/v1/snapshot/{name}`: one JPEG, for a surface with no video path.
 *
 * `name` is "sheet", "program" or a source id. `cacheBust` adds a timestamp,
 * which an <img> needs to fetch the same URL twice.
 */
export function snapshotUrl(
  origin: string,
  name: string,
  opts: { width?: number; token?: string | null; cacheBust?: boolean } = {},
): string {
  const u = new URL(`/api/v1/snapshot/${encodeURIComponent(name)}`, httpBase(origin));
  if (opts.width) u.searchParams.set("width", String(Math.round(opts.width)));
  if (opts.cacheBust) u.searchParams.set("t", String(Date.now()));
  if (opts.token) u.searchParams.set("token", opts.token);
  return u.toString();
}

/**
 * `GET /mjpeg/{name}`: `multipart/x-mixed-replace`, one JPEG per part.
 *
 * `name` is "sheet", "program", "preview" or a source id. In a browser this is
 * an `<img src>` and nothing more; outside one it is an HTTP body read part by
 * part.
 */
export function mjpegUrl(
  origin: string,
  name: string,
  opts: { width?: number; fps?: number; token?: string | null } = {},
): string {
  const u = new URL(`/mjpeg/${encodeURIComponent(name)}`, httpBase(origin));
  if (opts.width) u.searchParams.set("width", String(Math.round(opts.width)));
  if (opts.fps) u.searchParams.set("fps", String(Math.round(opts.fps)));
  if (opts.token) u.searchParams.set("token", opts.token);
  return u.toString();
}

/** `POST /whep/{name}`: the WebRTC offer endpoint, for audio and low latency. */
export function whepUrl(origin: string, name: string, token?: string | null): string {
  const u = new URL(`/whep/${encodeURIComponent(name)}`, httpBase(origin));
  if (token) u.searchParams.set("token", token);
  return u.toString();
}

/** The core's address as http or https, whatever scheme was handed in. */
export function httpBase(origin: string): string {
  const u = base(origin);
  u.protocol = u.protocol === "https:" || u.protocol === "wss:" ? "https:" : "http:";
  return u.origin;
}
