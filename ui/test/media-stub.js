// A client for the Media panel's tests: a small library with folders and
// tags, every call recorded, and a move that can be refused once as if a
// file were on programme.

export const FOLDERS = ["Ads", "Sermons", "Sermons/2026"];

export function items() {
  const item = (name, tags, extra) => ({
    name,
    path: `/media/${name}`,
    size_bytes: 1000,
    duration_ms: 5000,
    has_video: true,
    has_audio: true,
    web_safe: true,
    folder: name.includes("/") ? name.slice(0, name.lastIndexOf("/")) : undefined,
    tags,
    ...extra,
  });
  return [
    item("welcome.mp4", []),
    item("Ads/promo.mp4", ["Choir"]),
    item("Sermons/sunday.mp4", ["Easter"]),
    item("Sermons/2026/easter.mp4", ["easter", "youth"]),
  ];
}

export function mediaStub({ refuseOnce = false } = {}) {
  const listeners = new Map();
  let refuse = refuseOnce;
  return {
    calls: [],
    state: {},
    transport: { base: location.origin, token: "tok" },
    call(method, params) {
      this.calls.push([method, params]);
      if (method === "media.list") return Promise.resolve({ dir: "/media", items: items(), folders: FOLDERS });
      if (method === "media.move" && refuse && !params.force) {
        refuse = false;
        const e = new Error("pic is on programme now");
        e.data = { sources: ["pic"], field: "force" };
        return Promise.reject(e);
      }
      if (method === "media.folder.add") return Promise.resolve({ path: params.name, made: true });
      return Promise.resolve({ moved: [] });
    },
    upload(name, file, progress, into) {
      this.calls.push(["upload", { name, ...(into || {}) }]);
      return Promise.resolve({ name });
    },
    on(event, fn) {
      listeners.set(event, fn);
      return () => listeners.delete(event);
    },
    onRender() {
      return () => {};
    },
    sent(method) {
      return this.calls.filter((c) => c[0] === method).map((c) => c[1]);
    },
  };
}
