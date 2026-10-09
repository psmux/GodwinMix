// The banner a page shows when it opens on a mixer that is already streaming,
// recording or receiving: left running in the background earlier, or started
// from another device. It says what, for how long, and offers Stop all and
// Keep. The header's pill stays red and counting either way.

import { el, fmtDuration } from "../../shell/dom.js";
import { toast } from "../../shell/toast.js";
import { outgoing, summary } from "./running-model.js";

/** "Still streaming from before: YouTube for 1:56:23, recording". */
export function bannerText(items) {
  const out = outgoing(items);
  const lead = out.some((i) => i.kind !== "recording")
    ? "Still streaming from before"
    : out.length
      ? "Still recording from before"
      : "Still receiving from before";
  return `${lead}: ${summary(items, fmtDuration)}`;
}

export function showBanner(client, items) {
  document.querySelector(".still-running")?.remove();
  const close = () => bar.remove();
  const stop = el("button.btn.danger", {
    text: outgoing(items).length ? "Stop all" : "See what is running",
    onclick: async () => {
      if (!outgoing(items).length) {
        close();
        (await import("./running.js")).openRunning(client);
        return;
      }
      stop.disabled = true;
      const { stopAll } = await import("./stop.js");
      if (await stopAll(client, items)) close();
      else stop.disabled = false;
    },
  });
  const bar = el("div.still-running", { role: "alert" }, [
    el("span.dot.live"),
    el("span.grow", { text: bannerText(items) }),
    el("button.btn", { text: "Details", onclick: () => import("./running.js").then((m) => m.openRunning(client)) }),
    stop,
    el("button.btn", {
      text: "Keep",
      onclick: () => {
        close();
        toast({ text: "Kept running. The red pill at the top shows it and stops it." });
      },
    }),
  ]);
  document.body.prepend(bar);
  return bar;
}
