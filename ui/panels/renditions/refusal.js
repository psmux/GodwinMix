// When the governor says no: what it would have cost, what is left, and
// each thing that would fit as a button that tries again with it.
//
// Shown inside the form that was refused rather than as a toast, because
// the next step is one of these buttons and a toast is gone in twelve
// seconds. A refusal is `data: {need, have, advice: [{text, request}]}`.

import { el } from "../../shell/dom.js";
import { refusalWords } from "./words.js";
import { describe } from "./model.js";
import { stylesheet } from "./shared.js";

export { isRefusal } from "./words.js";

/**
 * @param {Error & {data: object}} err
 * @param {(request: object) => Promise<boolean>} retry  resolves true when it went through
 */
export function refusalBox(err, retry) {
  stylesheet();
  const d = err.data || {};
  const words = refusalWords(d);
  const advice = Array.isArray(d.advice) ? d.advice : [];
  const buttons = advice.map((a) => adviceButton(a, retry));
  return el("div.rnd-refusal", { role: "alert" }, [
    el("strong", { text: "Not started, so nothing on air drops a frame." }),
    el("p", { text: `${words.need} ${words.room}`.trim() }),
    buttons.length ? el("p.rnd-dim", { text: "Any of these fits. Press one to start it instead:" }) : el("p.rnd-dim", { text: "Stop another output, or pick a smaller format above." }),
    buttons.length ? el("div.rnd-advice", {}, buttons) : null,
  ]);
}

function adviceButton(a, retry) {
  const d = describe(a.request);
  const detail = [d.size, d.fps, d.bitrate, d.codec].filter(Boolean).join(" · ");
  const b = el("button.btn.rnd-try", { type: "button", title: detail }, [el("span", { text: a.text || detail || "Try this" }), detail && a.text ? el("span.rnd-dim", { text: detail }) : null]);
  b.onclick = async () => {
    const all = [...b.parentNode.children];
    for (const x of all) x.disabled = true;
    const ok = await retry(a.request).catch(() => false);
    if (!ok) for (const x of all) x.disabled = false;
  };
  return b;
}

/**
 * Put a refusal in a form, in place of the one before it, or clear it.
 * `slot` is an element the form keeps for it.
 */
export function showRefusal(slot, err, retry) {
  slot.replaceChildren(err ? refusalBox(err, retry) : "");
  slot.hidden = !err;
  if (err) slot.scrollIntoView({ block: "nearest" });
}
