// A stand in for a core answering `plugin.gallery`, `plugin.add`,
// `plugin.update`, `plugin.remove` and `task.get`, in the shapes protocol.json
// gives them. An install flips the plugin to installed, so a second
// `plugin.gallery` answers what a real core would.

import { RpcError } from "../client/errors.js";

const base = {
  tier: "custom", kinds: ["source"], screenshots: [], tags: [], versions: ["1.0.0"], version: "1.0.0",
  bundled: false, signed: false, installed: false, update: false, runs_here: true,
  fit: "Runs here", fit_detail: "There is a build of it for this computer.", marketplace: "godwinmix-plugins",
};

export function plugins() {
  return [
    { ...base, name: "camera", install: "camera", title: "Camera", tier: "gold", kinds: ["source", "device"], bundled: true, stars: 8,
      description: "A USB or built in camera as a source: v4l2 on Linux. More words after.", author: "GodwinMix", license: "Apache-2.0",
      repository: "https://github.com/psmux/GodwinMix/tree/main/plugins/camera", fit_detail: "It comes with GodwinMix, so it installs with no download." },
    { ...base, name: "clock", install: "clock", title: "Clock", stars: 1240, downloads: 98000, tags: ["time"],
      description: "Draws the current time, large, on a solid background.", thumbnail: "data:image/gif;base64,R0lGODlhAQABAAAAACw=",
      screenshots: ["data:image/gif;base64,R0lGODlhAQABAAAAACw="], updated: "2026-10-01T09:00:00Z", author: "someone" },
    { ...base, name: "macfx", install: "macfx", title: "Mac effects", kinds: ["filter"], runs_here: false, fit: "Not for Windows",
      fit_detail: "It is built for macos-aarch64 only.", description: "Colour effects for a Mac." },
    { ...base, name: "ndi", install: "ndi", title: "NDI", tier: "gold", kinds: ["source", "output"], installed: true, installed_version: "0.9.0",
      update: true, version: "1.0.0", versions: ["0.9.0", "1.0.0"], description: "NDI in and out." },
  ];
}

export class GalleryStub {
  constructor(opts = {}) {
    this.calls = [];
    this.list = plugins();
    this.fail = opts.fail || {};
    this.offline = !!opts.offline;
    this.tasks = new Map();
    this.state = {};
  }

  async call(method, params = {}) {
    this.calls.push([method, params]);
    if (method === "plugin.gallery") {
      const fetched = Math.floor(Date.now() / 1000) - (this.offline ? 7200 : 30);
      return { platform: "windows-x86_64", offline: this.offline, plugins: this.list.map((p) => ({ ...p })),
        marketplaces: [{ name: "godwinmix-plugins", source: "psmux/godwinmix-plugins", fetched, ...(this.offline ? { problem: "no route" } : {}) }] };
    }
    if (method === "plugin.add") return this.task(params.source, "add");
    if (method === "plugin.update") return this.task(params.id, "update");
    if (method === "plugin.remove") {
      const p = this.find(params.id);
      Object.assign(p, { installed: false, update: false, installed_version: undefined });
      return { removed: p.name, provides: [], tools: [] };
    }
    if (method === "task.get") return this.tasks.get(params.task_id);
    throw new RpcError(-32601, `no method ${method}`);
  }

  find(name) {
    const p = this.list.find((q) => q.name === name);
    if (!p) throw new RpcError(-32004, `there is no plugin called \`${name}\`. Search the marketplaces.`);
    return p;
  }

  task(name, act) {
    const id = `t${this.tasks.size + 1}`;
    const why = this.fail[name];
    if (why) this.tasks.set(id, { task_id: id, state: "failed", error: why });
    else {
      const p = this.find(name);
      Object.assign(p, { installed: true, update: false, installed_version: p.version });
      this.tasks.set(id, { task_id: id, state: "done", result: {} });
    }
    return { task_id: id, poll_interval_ms: 1 };
  }
}
