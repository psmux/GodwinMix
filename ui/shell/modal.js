// Modals, in the `modal` slot.
//
// Not `<dialog>`: WebKitGTK only shipped it in 2.36 and the polyfill costs more
// than the twenty lines here. A scrim, a box, focus moved in, focus put back,
// Escape closes. Nothing else.

import { el, on } from "./dom.js";

/**
 * @param {{title: string, body: Node, footer?: Node[], wide?: boolean,
 *          onClose?: () => void}} opts
 * @returns {{el, close}}
 */
export function modal(opts) {
  const slot = document.querySelector(".slot-modal") || document.body;
  const returnFocus = document.activeElement;

  const box = el("div.dialog", { role: "dialog", "aria-modal": "true", "aria-label": opts.title });
  if (opts.wide) box.style.maxWidth = "min(1100px, 96vw)";
  const head = el("header", {}, [
    el("h2.grow", { text: opts.title }),
    el("button.btn.icon", { text: "×", title: "Close", "aria-label": "Close", onclick: () => close() }),
  ]);
  box.appendChild(head);
  const body = el("div.body", {}, [opts.body]);
  box.appendChild(body);
  if (opts.footer) box.appendChild(el("footer", {}, opts.footer));

  const scrim = el("div.scrim", {
    onpointerdown: (e) => {
      if (e.target === scrim) close();
    },
  }, [box]);
  slot.appendChild(scrim);

  const offEsc = on(window, "keydown", (e) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      close();
    }
    if (e.key === "Tab") trap(e, box);
  }, true);

  function close() {
    offEsc();
    scrim.remove();
    if (opts.onClose) opts.onClose();
    if (returnFocus && returnFocus.focus) returnFocus.focus();
  }

  const first = box.querySelector("input, select, textarea, button.primary, button");
  if (first) first.focus();

  return { el: box, body, close };
}

const FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

function trap(e, box) {
  const items = [...box.querySelectorAll(FOCUSABLE)].filter((n) => n.offsetParent !== null);
  if (!items.length) return;
  const first = items[0];
  const last = items[items.length - 1];
  if (e.shiftKey && document.activeElement === first) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && document.activeElement === last) {
    e.preventDefault();
    first.focus();
  }
}

/** A yes or no, for the destructive things, with the id in the question. */
export function confirmModal(question, confirmLabel) {
  return new Promise((resolve) => {
    let answered = false;
    const done = (v) => {
      answered = true;
      m.close();
      resolve(v);
    };
    const m = modal({
      title: "Are you sure?",
      body: el("p", { text: question, style: { margin: "0" } }),
      footer: [
        el("button.btn", { text: "Cancel", onclick: () => done(false) }),
        el("button.btn.primary", { text: confirmLabel || "Yes", onclick: () => done(true) }),
      ],
      onClose: () => {
        if (!answered) resolve(false);
      },
    });
  });
}
