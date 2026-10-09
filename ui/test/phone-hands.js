// What an acceptance test with a phone in hand found: the composer's tools
// out of sight, and a keyboard over every form the moment it opened.

import { toolTabs } from "../panels/composer/tool-tabs.js";
import { opening } from "../shell/modal.js";

function group(name, buttons) {
  const g = document.createElement("div");
  g.className = "composer-group";
  g.innerHTML = `<span class="sm faint">${name}</span>` + buttons.map((b) => `<button class="btn sm">${b}</button>`).join("");
  return g;
}

function box(html) {
  const b = document.createElement("div");
  b.innerHTML = html;
  return b;
}

/** Run `fn` as though the main pointer were a finger, or a mouse. */
function pointer(finger, fn) {
  const real = window.matchMedia;
  window.matchMedia = (q) => ({ matches: q === "(pointer: coarse)" ? finger : real.call(window, q).matches, media: q });
  try { return fn(); } finally { window.matchMedia = real; }
}

/** A rename typed into a tile's name and ended with Enter, then the blur. */
async function renameOnce() {
  const { default: SourcesPanel } = await import("../panels/sources/panel.js");
  const name = document.createElement("span");
  name.textContent = "Cam";
  document.body.append(name);
  const calls = [];
  const panel = Object.assign(Object.create(SourcesPanel.prototype), {
    tiles: new Map([["cam", { name }]]),
    // Never answers, so nothing after the call (undo, a toast) runs in the test page.
    client: { state: {}, call: (_method, params) => { calls.push(params.name); return new Promise(() => {}); } },
    render() {},
  });
  try {
    panel.beginRename("cam");
    name.textContent = "Stage left";
    name.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
    name.blur();
    name.dispatchEvent(new FocusEvent("blur"));
    await new Promise((r) => setTimeout(r, 0));
  } finally {
    name.remove();
  }
  return calls;
}

export async function phoneHandsTests(test, eq, ok) {
  const renames = await renameOnce();
  test("a rename ended with Enter is sent once, not again by the blur that ending it causes", () => {
    eq(renames, ["Stage left"]);
  });

  test("the composer's chips show one group of tools at a time, named as the groups are", () => {
    const groups = [group("Align", ["Left", "Right"]), group("Size", ["Fit the canvas"])];
    const chips = toolTabs(groups);
    eq([...chips.children].map((c) => c.textContent), ["Align", "Size"]);
    eq(groups.map((g) => g.classList.contains("on")), [true, false]);
    eq(chips.children[0].getAttribute("aria-selected"), "true");
    chips.children[1].click();
    eq(groups.map((g) => g.classList.contains("on")), [false, true]);
    eq([...chips.children].map((c) => c.getAttribute("aria-selected")), ["false", "true"]);
  });

  test("a dialog opening under a finger focuses its one field, and a form's button instead", () => {
    const prompt = box(`<input type="text"><footer><button class="btn primary">Rename</button></footer>`);
    const form = box(`<input type="text"><input type="checkbox"><input type="password"><footer><button class="btn">Cancel</button><button class="btn primary">Add</button></footer>`);
    pointer(true, () => {
      eq(opening(prompt).tagName, "INPUT");
      eq(opening(form).textContent, "Add");
    });
    pointer(false, () => {
      eq(opening(form).tagName, "INPUT");
      ok(opening(form) === form.querySelector("input"), "a mouse gets the first field");
    });
  });
}
