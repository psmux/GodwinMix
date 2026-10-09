// What a phone operator could not reach: a scene's sources, the channels,
// and every tab once this browser's camera was folded over the tab bar.

import { SceneStrip } from "../panels/sources/scene-strip.js";
import { focusedScene, setFocusedScene } from "../shell/focus.js";
import { place } from "../shell/float-drag.js";
import { BrowserDock } from "../panels/sources/browser-dock.js";

const SCENES = [{ id: "wide", name: "Wide", items: 2 }, { id: "talk", name: "Talk", items: 0 }];

export function phoneReachTests(test, eq, ok) {
  test("the scene row on Sources lights the scene in hand and a chip moves the focus, nothing else", () => {
    const before = focusedScene();
    const strip = new SceneStrip();
    document.body.append(strip.node);
    try {
      strip.paint(SCENES, "wide");
      eq([...strip.node.children].map((c) => c.getAttribute("aria-selected")), ["true", "false"]);
      eq(strip.node.children[0].textContent, "Wide2");
      strip.node.children[1].click();
      eq(focusedScene(), "talk");
      strip.paint(SCENES.slice(0, 1), "wide");
      ok(strip.node.hidden, "one scene needs no row");
    } finally {
      strip.node.remove();
      setFocusedScene(before);
    }
  });

  test("a moved card stays inside the window and above a tab bar along the bottom", () => {
    const bar = document.createElement("nav");
    bar.className = "phone-nav";
    Object.assign(bar.style, { position: "fixed", left: "0", right: "0", bottom: "0", height: "60px" });
    const card = document.createElement("section");
    Object.assign(card.style, { position: "fixed", width: "200px", height: "50px" });
    document.body.append(bar, card);
    try {
      place(card, -40, innerHeight + 100);
      const r = card.getBoundingClientRect();
      eq(r.left, 8);
      ok(r.bottom <= bar.getBoundingClientRect().top, "clear of the tab bar");
      eq(card.style.right, "auto");
    } finally {
      bar.remove();
      card.remove();
    }
  });

  test("a folded camera card stays folded when the same error comes back on a retry", () => {
    let shown = 0;
    const node = document.createElement("section");
    const dock = Object.assign(Object.create(BrowserDock.prototype), {
      node, pub: { setVisible() {} }, paint() {}, show() { shown++; },
    });
    dock.stateChanged({ state: "reconnecting", error: "already live" });
    eq(shown, 1);
    dock.fold(true);
    dock.stateChanged({ state: "reconnecting", error: "already live" });
    eq(shown, 1);
    dock.stateChanged({ state: "reconnecting", error: "no route" });
    eq(shown, 2);
  });
}
