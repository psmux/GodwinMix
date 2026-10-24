// Switching shows the way a browser switches tabs: one click on another
// show's tab, a double click, a double tap, Ctrl+Tab and Ctrl+Shift+Tab, the
// keys on a Mac, and a redraw that keeps the keyboard on its tab. Each case
// gets a row of its own, since a row that has switched waits for the page
// to move and takes no second switch.

import { showStub } from "./shows-stub.js";

const ctrl = (k, extra = {}) => new KeyboardEvent("keydown", { key: k, ctrlKey: true, bubbles: true, cancelable: true, ...extra });

export async function switchTests(test, eq, ok, { row, key, until, wait }) {
  const actions = await import("../shell/show-actions.js");
  const { stepOf } = await import("../shell/show-tabs.js");
  const went = [];
  actions.nav.go = (url) => went.push(url);
  const fresh = async () => {
    went.length = 0;
    const node = await row(showStub());
    return { node, tab: (id) => node.querySelector(`[data-show="${id}"]`) };
  };

  let r = await fresh();
  r.tab("kids").click();
  await until(() => went.length);
  test("one click on another show loads the page on that show", () => ok(/[?&]show=kids\b/.test(went[0] || ""), went[0]));
  test("the tab shows as chosen at once, before the page has moved", () => {
    eq(r.tab("kids").getAttribute("aria-selected"), "true");
    eq(r.tab("main").getAttribute("aria-selected"), "false");
  });
  r.node.remove();

  r = await fresh();
  const at = r.tab("kids");
  at.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
  at.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 2 }));
  at.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, detail: 2 }));
  await wait(400);
  test("a double click on another show switches once and renames nothing", () => {
    eq(went.length, 1);
    ok(/[?&]show=kids\b/.test(went[0] || ""), went[0]);
    ok(!r.node.querySelector(".showtab-input"), "no rename box");
  });
  r.node.remove();

  r = await fresh();
  window.dispatchEvent(ctrl("Tab"));
  await until(() => went.length);
  test("Ctrl+Tab goes to the next show", () => ok(/[?&]show=kids\b/.test(went[0] || ""), went[0]));
  r.node.remove();

  r = await fresh();
  window.dispatchEvent(ctrl("Tab", { shiftKey: true }));
  await until(() => went.length);
  test("Ctrl+Shift+Tab goes to the show before, round from the first to the last", () => ok(/[?&]show=loop\b/.test(went[0] || ""), went[0]));
  r.node.remove();

  test("the keys a browser uses for its own tabs, and nothing else, step along the shows", () => {
    eq(stepOf(ctrl("PageDown")), 1);
    eq(stepOf(ctrl("PageUp")), -1);
    eq(stepOf(new KeyboardEvent("keydown", { metaKey: true, shiftKey: true, code: "BracketRight", key: "}" }), true), 1);
    eq(stepOf(new KeyboardEvent("keydown", { metaKey: true, shiftKey: true, code: "BracketLeft", key: "{" }), true), -1);
    eq(stepOf(new KeyboardEvent("keydown", { key: "Tab" })), 0);
    eq(stepOf(new KeyboardEvent("keydown", { key: "Tab", ctrlKey: true, altKey: true })), 0);
    eq(stepOf(new KeyboardEvent("keydown", { metaKey: true, shiftKey: true, code: "BracketRight", key: "}" }), false), 0);
  });

  r = await fresh();
  r.tab("main").focus();
  key(r.tab("main"), "ArrowRight");
  await until(() => document.activeElement === r.tab("kids"));
  // Enter or Space on a focused button is a click.
  r.tab("kids").click();
  await until(() => went.length);
  test("arrow keys move along the tabs, and Enter on the one reached switches to it", () => ok(/[?&]show=kids\b/.test(went[0] || ""), went[0]));
  r.node.remove();

  r = await fresh();
  const wall = await import("../panels/wall/view.js");
  wall.toggleWall(showStub());
  await until(() => document.querySelector("section.wall"));
  const main = r.tab("main");
  main.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
  main.dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 2 }));
  main.dispatchEvent(new MouseEvent("dblclick", { bubbles: true, detail: 2 }));
  await until(() => !document.querySelector("section.wall"));
  await wait(400);
  test("with the wall open, a click or double click on this show's tab closes the wall, and renames nothing", () => {
    ok(!document.querySelector("section.wall"), "the wall is gone");
    ok(!r.node.querySelector(".showtab-input"), "no rename box");
    eq(went.length, 0);
    ok(!document.querySelector(".ctx-menu, [role=menu]"), "no tab menu");
  });
  r.node.remove();

  r = await fresh();
  const headend = await import("../panels/headend/view.js");
  headend.toggleHeadend(showStub());
  await until(() => document.querySelector("section.he"));
  r.tab("kids").dispatchEvent(new MouseEvent("click", { bubbles: true, detail: 1 }));
  await until(() => went.length);
  test("with the headend page open, a click on another show's tab closes it and switches", () => {
    ok(!document.querySelector("[data-cover]"), "no page left over the show");
    ok(/[?&]show=kids\b/.test(went[0] || ""), went[0]);
  });
  r.node.remove();

  const stub = showStub();
  const node = await row(stub);
  node.querySelector('[data-show="kids"]').focus();
  stub.emit("event", { name: "show.changed", params: { show: stub.shows[2] } });
  await wait(120);
  test("a redraw keeps the keyboard on the same show's tab", () => eq(document.activeElement && document.activeElement.dataset.show, "kids"));
  node.remove();
  actions.nav.go = (url) => location.assign(url);
}
