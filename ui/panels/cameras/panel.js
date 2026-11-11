// The camera window: every camera, the next shot, and taking it live, in
// three steps a volunteer can follow, with the switcher's own controls one
// toggle away. It reads and paints only while it is on screen.

import { el } from "../../shell/dom.js";
import { toast } from "../../shell/toast.js";
import { Model } from "./model.js";
import { Drive } from "./drive.js";
import { strip } from "./strip.js";
import { frameStep } from "./frame.js";
import { takeStep } from "./take.js";
import { atemBar } from "./atem-bar.js";
import { proView } from "./pro.js";
import { gamepads } from "./gamepad.js";
import { header, saved, save } from "./head.js";
import { switchClient } from "./switch.js";

export class CameraView {
  constructor(host, client) {
    this.host = host;
    this.real = client;
    this.model = new Model(client);
    this.running = false;
    const m = this.model;
    m.pro = !!saved().pro;
    this.drive = new Drive({
      move: (c, p, t) => m.api.move(c, p, t),
      stop: (c) => m.api.stop(c),
      zoom: (c, s) => m.api.zoom(c, s),
    });
    this.drive.onError = (e) => this.fail(e);
    const act = (fn) => this.act(fn);
    const on = {
      pick: (id) => m.pick(id),
      demo: () => (this.head.set({ demo: true, pro: m.pro }), this.setDemo(true)),
      follow: (id, person) => (m.pick(id), act(() => m.api.followPerson(id, person))),
    };
    this.head = header({ demo: (v) => this.setDemo(v), pro: (v) => this.setPro(v) });
    this.strip = strip(m, on);
    this.frame = frameStep(m, this.drive, act);
    this.take = takeStep(m, act);
    this.atem = atemBar(m, act);
    this.pro = proView(m, act);
    this.pad = gamepads({
      move: (p, t) => this.drive.move(p, t),
      zoom: (v) => this.drive.zoom(v),
      stop: () => this.drive.stopAll(),
      cut: () => this.take.run("cut"),
      fade: () => this.take.run("fade"),
      step: (d) => this.step(d),
    });
    this.root = el("div.cam-panel", {}, [
      this.head.root,
      this.atem.root,
      this.pro.root,
      this.strip.root,
      el("div.cam-steps", {}, [this.frame.root, this.take.root]),
    ]);
    host.append(this.root);
    this.offModel = m.on(() => this.later());
    this.resize = new ResizeObserver(() => this.fit());
    this.resize.observe(this.root);
    this.head.set({ demo: !!saved().demo, pro: m.pro });
    this.setDemo(!!saved().demo);
  }

  fit() {
    const w = this.root.clientWidth;
    this.root.classList.toggle("wide", w >= 900);
    this.root.classList.toggle("narrow", w < 600);
  }

  later() {
    if (this.queued) return;
    this.queued = requestAnimationFrame(() => {
      this.queued = 0;
      this.sync();
    });
  }

  sync() {
    this.strip.sync();
    this.frame.sync();
    this.take.sync();
    this.atem.sync();
    this.pro.sync();
    this.head.status(this.model);
  }

  act(fn) {
    return Promise.resolve()
      .then(fn)
      .catch((e) => this.fail(e))
      .finally(() => this.model.refresh());
  }

  fail(e) {
    toast({ kind: "warning", text: (e && e.message) || String(e) });
  }

  step(dir) {
    const cams = this.model.cams;
    if (!cams.length) return;
    const i = cams.findIndex((c) => c.id === this.model.next);
    this.model.pick(cams[(i + dir + cams.length) % cams.length].id);
  }

  setDemo(on) {
    return switchClient(this, on);
  }

  setPro(on) {
    save({ pro: on });
    this.model.pro = on;
    this.root.classList.toggle("pro", on);
    this.later();
  }

  start() {
    if (this.running) return;
    this.running = true;
    if (this.client && this.client.start) this.client.start();
    this.offEvents = this.client && this.client.listen ? this.client.listen("program.*") : null;
    this.model.start();
    this.strip.start();
    this.pad.start();
    this.root.classList.toggle("pro", this.model.pro);
  }

  stop() {
    if (!this.running) return;
    this.running = false;
    this.drive.stopAll();
    this.frame.release();
    this.pad.stop();
    this.model.stop();
    this.strip.stop();
    this.offEvents?.();
    if (this.client && this.client.stop) this.client.stop();
  }

  destroy() {
    this.stop();
    this.drive.destroy();
    this.pad.destroy();
    this.offModel();
    this.offMix?.();
    this.resize.disconnect();
  }
}
