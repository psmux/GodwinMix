// One channel's detail, in a drawer at the side (a sheet from the bottom
// on a phone): a player for its HLS, every track in and out, the
// transcoder settings, the input's history since the page opened, and
// Restart and Disable. The player is made once when the drawer opens; the
// rest is redrawn each second from what the view already holds.

import { el } from "../../shell/dom.js";
import { errorToast, toast } from "../../shell/toast.js";
import { kbps } from "../wall/model.js";
import { WORD, numbered, stateOf } from "./model.js";
import { player } from "./player.js";
import { history, inputTracks, outputTracks, transcoder } from "./parts.js";

const section = (title, body) => el("section.he-dsec", {}, [el("h4", { text: title }), body]);

export function drawer(view) {
  const node = el("aside.he-drawer", { "aria-label": "Channel detail", hidden: true });
  let id = null;
  let play = null;
  let armed = 0;
  const head = el("div.he-dhead");
  const body = el("div.he-dbody");
  const acts = el("div.he-dacts");
  const slot = el("div.he-dplay");
  node.append(head, slot, acts, body);

  const close = () => {
    if (play) play.stop();
    play = null;
    id = null;
    node.hidden = true;
    view.selected = null;
    view.later();
  };

  async function act(what) {
    const show = view.data.find(id);
    if (!show) return;
    if (what === "stop" && armed < Date.now()) {
      armed = Date.now() + 4000;
      return draw();
    }
    armed = 0;
    try {
      await view.data.power(id, what);
      toast({ text: what === "stop" ? `${show.name} is off. Enable starts it again.` : what === "start" ? `${show.name} is starting.` : `${show.name} was stopped and started again.` });
    } catch (e) {
      errorToast(e, `${show.name} did not ${what === "stop" ? "stop" : "start"}`);
    }
  }

  node.addEventListener("click", (e) => {
    const b = e.target.closest("[data-dact]");
    if (!b) return;
    if (b.dataset.dact === "close") close();
    else act(b.dataset.dact);
  });

  function draw() {
    const show = id && view.data.find(id);
    if (!show) return;
    const st = view.data.stats.get(id);
    const state = stateOf(show, st);
    const n = numbered(show);
    const i = (st && st.input) || {};
    head.replaceChildren(
      el("span.he-dnum", { text: n.num }),
      el("div.he-dtitle", {}, [el("h3", { text: n.title }), el("span.he-sub", { text: show.input ? `${show.input.uri}${show.input.program ? `, program ${show.input.program}` : ""}` : "" })]),
      el(`span.he-dstate.s-${state}`, { text: WORD[state] }),
      el("button.btn.icon", { type: "button", "data-dact": "close", "aria-label": "Close the detail", title: "Close (Escape)", text: "×" }),
    );
    const on = show.state === "running" || show.state === "live" || show.state === "starting";
    acts.replaceChildren(
      el("span.he-dnums", { text: [kbps(i.kbps || 0), i.cc_errors ? `${i.cc_errors} CC errors in all` : "", i.packets_lost ? `${i.packets_lost} packets lost` : "", i.keyframe_ms ? `keyframe every ${(i.keyframe_ms / 1000).toFixed(1)} s` : ""].filter(Boolean).join(" · ") }),
      on ? el("button.btn", { type: "button", "data-dact": "restart", text: "Restart" }) : null,
      on ? el("button.btn.he-danger", { type: "button", "data-dact": "stop", text: armed > Date.now() ? "Press again to disable" : "Disable" }) : el("button.btn.primary", { type: "button", "data-dact": "start", text: "Enable" }),
    );
    body.replaceChildren(
      section("Input tracks", inputTracks(st)),
      section("Outputs", outputTracks(show, st)),
      section("Transcoder", transcoder(show, st)),
      section("Input history", history(view.data.series.get(id))),
    );
  }

  return {
    node,
    get id() { return id; },
    open(showId) {
      if (id === showId) return;
      if (play) play.stop();
      id = showId;
      node.hidden = false;
      const show = view.data.find(id);
      play = player(view.client, show);
      slot.replaceChildren(play.node);
      draw();
    },
    draw,
    close,
  };
}
