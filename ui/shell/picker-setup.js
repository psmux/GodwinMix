// A picker category whose first party plugin is not set up yet.
//
// Opening the category is the request: the mixer installs the plugin from
// the copy it carries (or switches it back on) and the devices are looked
// for once it is ready. The person reads one plain sentence while it runs and
// sees a button only if it did not finish.

import { el } from "./dom.js";
import { errorText } from "./toast.js";

/**
 * @param client  the connected client
 * @param cat     a CATEGORIES entry with `plugin`
 * @param onReady called once, when the plugin is ready, to look for devices
 */
export function setupBlock(client, cat, onReady) {
  const piece = cat.plugin.name;
  const line = el("p.dim", { text: cat.plugin.line, style: { marginTop: "0" } });
  const again = el("button.btn.primary", { text: "Try again", style: { display: "none" } });
  const node = el("div.col", {}, [line, el("div.row", {}, [again, el("span.grow")])]);
  let done = false;
  const off = client.on ? client.on("setup", (s) => show(s)) : () => {};
  function show(s) {
    if (!s || s.piece !== piece || done) return;
    if (!node.isConnected && s.state !== "running") return off();
    line.textContent = s.message;
    again.style.display = s.state === "failed" ? "" : "none";
    if (s.state === "ready") {
      done = true;
      off();
      onReady();
    }
  }
  function start() {
    again.style.display = "none";
    client
      .call("setup.start", { piece })
      .then(show)
      .catch((e) => {
        line.textContent = errorText(e, `${cat.title} could not be set up`);
        again.style.display = "";
      });
  }
  again.onclick = start;
  start();
  return node;
}
