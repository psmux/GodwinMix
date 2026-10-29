// Driving the prompter from the desk: play, speed, markers, the place, and
// how the words look on every screen.

import { el } from "../../shell/dom.js";
import { positionAt } from "../../prompter/clock.js";

export function drive(client, failed) {
  let state = null;
  let timer = 0;
  const go = (method, params) => client.call(method, params || {}).then(update, failed);
  const play = el("button.btn.primary", { text: "Play", onclick: () => go(state && state.playing ? "prompter.pause" : "prompter.play") });
  const speed = el("span.num.prompter-speed", { text: "" });
  const title = el("strong.grow", { text: "Nothing on the prompter" });
  const markers = el("div.row.wrap.prompter-markers");
  const place = el("input.grow", { type: "range", min: 0, max: 1000, value: 0, "aria-label": "Place in the script",
    onchange: () => go("prompter.seek", { fraction: place.value / 1000 }) });
  const where = el("span.sm.dim.num");
  const look = lookControls(go);

  const root = el("div.col.prompter-drive", {}, [
    el("div.row", {}, [title, where]),
    el("div.row.wrap", {}, [
      el("button.btn", { text: "Top", onclick: () => go("prompter.seek", { row: 0 }) }),
      el("button.btn", { text: "Back", title: "The marker before", onclick: () => go("prompter.seek", { step: -1 }) }),
      play,
      el("button.btn", { text: "Next", title: "The next marker", onclick: () => go("prompter.seek", { step: 1 }) }),
      el("button.btn", { text: "Slower", onclick: () => go("prompter.speed", { by: -0.1 }) }),
      speed,
      el("button.btn", { text: "Faster", onclick: () => go("prompter.speed", { by: 0.1 }) }),
    ]),
    el("div.row", {}, [place]),
    markers,
    look.root,
  ]);

  function tick() {
    if (!state || !state.rows) return;
    const at = positionAt(state, Date.now());
    if (document.activeElement !== place) place.value = String(Math.round((at / state.rows) * 1000));
    where.textContent = `row ${Math.min(Math.floor(at) + 1, state.rows)} of ${state.rows}`;
  }

  function update(next) {
    if (!next || (state && next.revision < state.revision)) return;
    state = next;
    title.textContent = state.script ? state.title || state.script : "Nothing on the prompter";
    play.textContent = state.playing ? "Pause" : "Play";
    play.disabled = !state.script;
    speed.textContent = `${state.speed.toFixed(1)} rows/s`;
    markers.replaceChildren(...state.markers.map((m) => el("button.btn.sm", { text: m.name, onclick: () => go("prompter.seek", { row: m.row }) })));
    look.paint(state);
    tick();
    clearInterval(timer);
    timer = state.playing ? setInterval(tick, 250) : 0;
  }

  return { root, update, stop: () => clearInterval(timer) };
}

/** Text size, margin, cue line and mirror, for every screen at once. */
function lookControls(go) {
  const number = (label, min, max, step, key) => {
    const input = el("input.num", { type: "number", min, max, step, "aria-label": label, onchange: () => go("prompter.set", { [key]: Number(input.value) }) });
    return { input, field: el("label.row.sm", {}, [el("span.dim", { text: label }), input]) };
  };
  const size = number("Characters a row", 12, 80, 1, "line_chars");
  const margin = number("Margin %", 0, 30, 1, "margin");
  const cue = number("Cue line", 0.1, 0.9, 0.05, "cue");
  const mirror = el("input", { type: "checkbox", onchange: () => go("prompter.set", { mirror: mirror.checked }) });
  const root = el("div.row.wrap.prompter-look", {}, [size.field, margin.field, cue.field, el("label.row.sm", {}, [mirror, el("span", { text: "Mirror for glass" })])]);
  return {
    root,
    paint(s) {
      for (const [f, v] of [[size, s.line_chars], [margin, s.margin], [cue, s.cue]]) if (document.activeElement !== f.input) f.input.value = String(v);
      mirror.checked = !!s.mirror;
    },
  };
}
