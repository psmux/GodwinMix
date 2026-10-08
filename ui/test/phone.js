// The phone deck: one screen at a time, panels suspended rather than moved
// when their screen goes, and every panel without a tab reachable from More.
// Driven directly against a detached host, so this runs at any window width.

import { PhoneDeck } from "../shell/phone.js";
import { registerElement, instantiate, get } from "../shell/registry.js";

const KEY = "gmx.phone.page";

function rig() {
  const host = document.createElement("div");
  const root = document.createElement("section");
  host.append(root);
  document.body.append(host);
  return { host, root, workspace: { client: {}, root } };
}

export function phoneTests(test, eq, ok) {
  const calls = { connected: 0, disconnected: 0, active: [] };
  class Kept extends HTMLElement {
    static get panel() { return { id: "test/phone-kept", title: "Kept", slots: ["main"] }; }
    connectedCallback() { calls.connected++; }
    disconnectedCallback() { calls.disconnected++; }
    setWorkspaceActive(on) { calls.active.push(on); }
  }
  class Plain extends HTMLElement {
    static get panel() { return { id: "test/phone-plain", title: "Plain", slots: ["main"] }; }
  }
  class Chrome extends HTMLElement {
    static get panel() { return { id: "test/phone-header", title: "Header", slots: ["header"] }; }
  }
  if (!get("test/phone-kept")) { registerElement(Kept); registerElement(Plain); registerElement(Chrome); }

  const saved = localStorage.getItem(KEY);
  localStorage.removeItem(KEY);
  const { host, root, workspace } = rig();
  const deck = new PhoneDeck(workspace);
  try {
    deck.render();
    test("the phone deck opens on Live with a tab bar of five", () => {
      eq(deck.page, "live");
      eq([...host.querySelectorAll(".phone-tab")].map((b) => b.dataset.tab), ["live", "sources", "audio", "outputs", "more"]);
      eq(host.querySelector('.phone-tab[aria-current="true"]').dataset.tab, "live");
      ok(host.classList.contains("phone") && root.classList.contains("phone-deck"));
    });

    test("More lists every panel without a tab, and never the header", () => {
      const ids = deck.extras().map((p) => p.id);
      ok(ids.includes("test/phone-kept") && ids.includes("test/phone-plain"));
      ok(!ids.includes("test/phone-header"));
      ok(!ids.some((id) => ["core/program", "core/scenes", "core/sources", "core/audio", "core/outputs"].includes(id)));
      deck.go("more");
      ok([...root.querySelectorAll(".phone-card")].some((c) => c.textContent.includes("Kept")));
    });

    test("a panel opened from More is a screen of its own with a way back, and More stays lit", () => {
      deck.open("test/phone-kept");
      eq(deck.page, "panel:test/phone-kept");
      eq(host.querySelector('.phone-tab[aria-current="true"]').dataset.tab, "more");
      ok(root.querySelector(".phone-back"), "a back button");
      eq(root.querySelector(".phone-head h1").textContent, "Kept");
      ok(root.querySelector('[data-dock-panel="test/phone-kept"]'));
      eq(localStorage.getItem(KEY), "panel:test/phone-kept");
    });

    test("leaving a screen suspends a panel that can be, without moving it, and wakes the same one", () => {
      const node = deck.frames.get("test/phone-kept").made.node;
      deck.go("more");
      ok(deck.frames.get("test/phone-kept").element.hidden, "hidden, not removed");
      eq(calls.disconnected, 0);
      deck.open("test/phone-kept");
      ok(deck.frames.get("test/phone-kept").made.node === node, "the same element");
      eq(calls.active, [false, true]);
      eq(calls.connected, 1);
    });

    test("a panel that cannot be suspended is taken down when its screen goes", () => {
      deck.open("test/phone-plain");
      const element = deck.frames.get("test/phone-plain").element;
      deck.go("live");
      ok(!deck.frames.has("test/phone-plain") && !element.isConnected);
    });

    test("a page that names a panel no longer registered falls back to Live", () => {
      deck.page = "panel:test/phone-gone";
      deck.render();
      eq(deck.page, "live");
    });
  } finally {
    deck.destroy();
    host.remove();
    if (saved === null) localStorage.removeItem(KEY); else localStorage.setItem(KEY, saved);
  }

  test("taking the deck down leaves no tab bar and no phone classes", () => {
    ok(!host.querySelector(".phone-nav") && !host.classList.contains("phone") && !root.classList.contains("phone-deck"));
    eq(calls.disconnected, 1);
  });

  test("a panel with a remove method of its own is still removed from the page", () => {
    let asked = 0;
    class Remover extends HTMLElement {
      static get panel() { return { id: "test/phone-remover", title: "Remover", slots: ["main"] }; }
      remove() { asked++; }
    }
    if (!get("test/phone-remover")) registerElement(Remover);
    const made = instantiate("test/phone-remover", {}, {});
    document.body.append(made.node);
    made.destroy();
    ok(!made.node.isConnected);
    eq(asked, 0);
  });
}
