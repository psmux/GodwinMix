// First run: ask for the token once, and say what to do first.
//
// The old page used window.prompt, guarded by a flag so the two second poll did
// not reprompt forever. This asks in the page, with the reason on screen, and
// only when a request has come back 401.

import { el, on } from "./dom.js";
import { modal } from "./modal.js";
import { storeToken } from "../client/index.js";

let asking = null;

/**
 * @returns {Promise<string|null>} the token the operator typed, or null
 */
export function askForToken(reason) {
  if (asking) return asking;
  asking = new Promise((resolve) => {
    const input = el("input", { type: "password", placeholder: "Paste the token", autocomplete: "off" });
    let answered = false;
    const save = el("button.btn.primary", {
      text: "Connect",
      onclick: () => {
        const value = input.value.trim();
        if (!value) return;
        answered = true;
        storeToken(value);
        m.close();
        resolve(value);
      },
    });
    on(input, "keydown", (e) => {
      if (e.key === "Enter") save.click();
    });
    const m = modal({
      title: "This mixer needs a token",
      body: el("div", {}, [
        el("p.dim", {
          text:
            reason ||
            "The mixer was started with a token, so it will not answer without one. It is the value of GODWINMIX_TOKEN, or the token line in the config file.",
          style: { marginTop: "0" },
        }),
        el("div.form", {}, [el("div.field", {}, [el("label", {}, [el("span.lbl", { text: "Token" }), input])])]),
      ]),
      footer: [
        el("button.btn", { text: "Not now", onclick: () => m.close() }),
        save,
      ],
      onClose: () => {
        asking = null;
        if (!answered) resolve(null);
      },
    });
    input.focus();
  });
  return asking;
}

/**
 * The empty state. Three sentences, in the order a first time user does them.
 * Shown in the tray when there are no sources at all.
 */
export function emptyState(onAdd) {
  return el("div.empty", {}, [
    el("div", {}, [
      el("h2", { text: "Nothing is set up yet" }),
      el("ol", {}, [
        el("li", { text: "Add a source: a camera, a file, a web page or an incoming stream." }),
        el("li", { text: "Tap its tile to put it on air. The picture at the top is what your audience sees." }),
        el("li", { text: "Add an output to send that picture somewhere." }),
      ]),
      el("button.btn.primary", { text: "Add a source", onclick: onAdd }),
    ]),
  ]);
}
