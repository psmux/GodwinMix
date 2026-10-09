// Stopping what runs, from the What is running panel, the banner and the
// Outputs rows: one question when the thing is live, then the one call the
// item names. Loaded the first time something is stopped.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { errorToast, toast } from "../../shell/toast.js";
import { confirmWording, outgoing, stopAllWording } from "./running-model.js";

/** Ask `wording`; true when the person said yes. */
export function ask(wording) {
  return new Promise((resolve) => {
    let answered = false;
    const done = (yes) => {
      answered = true;
      m.close();
      resolve(yes);
    };
    const m = modal({
      title: wording.title,
      body: el("p", { text: wording.body, style: { margin: "0" } }),
      footer: [
        el("button.btn", { text: "Keep it running", onclick: () => done(false) }),
        el("button.btn.danger", { text: wording.yes, onclick: () => done(true) }),
      ],
      onClose: () => answered || resolve(false),
    });
  });
}

/** Stop one item, asking first when it is live. True when it was stopped. */
export async function stopItem(client, item) {
  const wording = confirmWording(item);
  if (wording && !(await ask(wording))) return false;
  try {
    await client.call(item.stop.method, item.stop.params);
    toast({ text: stoppedText(item) });
    return true;
  } catch (e) {
    errorToast(e, "Stop " + item.title);
    return false;
  }
}

/** Stop everything outgoing, after one question. True when all of it stopped. */
export async function stopAll(client, items) {
  const out = outgoing(items);
  if (!out.length) return true;
  if (out.some((i) => i.live) && !(await ask(stopAllWording(items)))) return false;
  let failed = 0;
  for (const item of out) {
    try {
      await client.call(item.stop.method, item.stop.params);
    } catch (e) {
      failed += 1;
      errorToast(e, "Stop " + item.title);
    }
  }
  if (!failed) toast({ text: "Everything stopped. Start streaming on Outputs sends again." });
  return failed === 0;
}

function stoppedText(item) {
  if (item.kind === "ingest") return `${item.title} is switched off. Switch it on again under Channels.`;
  if (item.key.startsWith("channel:")) return `Stopped ${item.title} on ${item.where}. Its switch under Channels sends again.`;
  if (item.kind === "recording") return "Recording stopped. The mixer is finishing the file.";
  return `Stopped ${item.title}. Its stream key is kept; Start streaming on Outputs sends again.`;
}
