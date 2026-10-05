// The Graphics gallery against a stubbed client: the model on its own, then
// the real panel drawn, filtered, and driven through Add and Take live.

import { label, badges, visible, nextStep, previewUrl, version } from "../panels/graphics/model.js";
import { promptFor, KINDS } from "../panels/graphics/prompts.js";

const settle = (ms) => new Promise((r) => setTimeout(r, ms));

/** The two dashes the house style keeps out of every word shown. */
const DASHES = [String.fromCharCode(0x2013), String.fromCharCode(0x2014)];

const ITEMS = [
  { id: "storm-strap", name: "Storm strap", kind: "template", zone: "lower-third", moves: false, transparent: true, origin: "agent", made_by: "opencode", tags: ["weather"], fields: [{ name: "headline", label: "Headline", type: "text", default: "Hi" }], saved: "2026-10-06T10:00:00Z" },
  { id: "blue-gradient", name: "Blue studio background", kind: "image", zone: "full", moves: false, transparent: false, origin: "shipped" },
  { id: "crawl", name: "Crawl", kind: "ticker", zone: "bottom", moves: true, transparent: true, origin: "uploaded", moving: "preview.webm", placed: ["crawl"] },
  { id: "studio", name: "Blue studio", kind: "set", zone: "full", moves: false, transparent: false, origin: "agent" },
  { id: "sting", name: "Sting", kind: "transition", zone: "overlay", moves: true, transparent: true, origin: "uploaded" },
];

function stub() {
  return {
    calls: [],
    transport: { base: location.origin, token: "tok" },
    call(method, params) {
      this.calls.push([method, params]);
      const answers = {
        "gallery.list": { items: ITEMS, dir: "graphics", total: ITEMS.length },
        "gallery.place": { id: params && params.id, scene: "Studio", visible: false, updated: false, new_scene: false, next: "" },
        "gallery.show": { scene: "Studio", item: "x", visible: !!(params && params.visible), took: false },
      };
      return answers[method] ? Promise.resolve(answers[method]) : Promise.reject(new Error(`no stub for ${method}`));
    },
    sent(method) {
      return this.calls.filter((c) => c[0] === method).map((c) => c[1]);
    },
  };
}

export async function galleryTests(test, eq, ok) {
  test("a card says what a graphic is for, not only what it is made of", () => {
    eq(ITEMS.map(label), ["Lower third", "Background", "Ticker", "Virtual set", "Transition"]);
    eq(badges(ITEMS[0]), ["Transparent", "By opencode"]);
    eq(badges(ITEMS[2]), ["Moves", "Transparent", "Imported", "In use"]);
  });
  test("the filters and the search find graphics by the words people use", () => {
    eq(visible(ITEMS, "lower", "").map((i) => i.id), ["storm-strap"]);
    eq(visible(ITEMS, "full", "").map((i) => i.id), ["blue-gradient"]);
    eq(visible(ITEMS, "all", "weather").map((i) => i.id), ["storm-strap"]);
    eq(visible(ITEMS, "mine", "").length, 4);
    eq(visible(ITEMS, "moving", "").map((i) => i.id), ["crawl", "sting"]);
  });
  test("one click adds, the next takes it live, a set makes a scene, a transition is not placed", () => {
    eq(nextStep(ITEMS[0], undefined).label, "Add");
    eq(nextStep(ITEMS[0], "placed").label, "Take live");
    eq(nextStep(ITEMS[0], "shown").label, "Take off");
    eq(nextStep(ITEMS[3], undefined).label, "Make scene");
    eq(nextStep(ITEMS[4], undefined), null);
  });
  test("a card's picture asks for its width with the token, and a change asks again", () => {
    const u = new URL(previewUrl({ transport: { base: "http://m:8080", token: "t" } }, ITEMS[0], 480));
    eq(u.pathname, "/api/v1/gallery/storm-strap/preview.jpg");
    eq(u.searchParams.get("width"), "480");
    eq(u.searchParams.get("token"), "t");
    ok(version(ITEMS[0]) !== version({ ...ITEMS[0], values: { headline: "New" } }), "new words, new address");
  });
  test("every prompt names the tools and the loop, for any agent", () => {
    for (const k of KINDS) {
      const p = promptFor(k.id, "a red evening news");
      ok(p.includes("save_graphic") && p.includes("preview_graphic") && p.includes("godwinmix tool"), k.id);
      ok(p.includes("a red evening news"), k.id);
      ok(!DASHES.some((d) => p.includes(d)), `${k.id} has a dash`);
    }
  });

  await import("../panels/graphics/panel.js");
  const client = stub();
  const view = document.createElement("gmx-graphics-view");
  view.setClient(client);
  view.style.display = "block";
  document.body.appendChild(view);
  await settle(60);
  const cards = [...view.querySelectorAll(".gx-card")];
  test("the gallery draws a card for every graphic and asks only for gallery.list", () => {
    eq(cards.length, ITEMS.length);
    eq(client.calls.map((c) => c[0]), ["gallery.list"]);
    ok(cards[0].querySelector("img").dataset.src.includes("/preview.jpg"));
  });
  test("a graphic already in use starts at Take live", () => {
    eq(view.querySelector('[data-id="crawl"] .gx-main').textContent, "Take live");
  });
  const main = view.querySelector('[data-id="storm-strap"] .gx-main');
  main.click();
  await settle(30);
  test("Add places the graphic and the button moves on to Take live", () => {
    eq(client.sent("gallery.place")[0], { id: "storm-strap" });
    eq(main.textContent, "Take live");
  });
  main.click();
  await settle(30);
  test("Take live shows it on air", () => {
    eq(client.sent("gallery.show")[0], { id: "storm-strap", visible: true });
    eq(main.textContent, "Take off");
  });
  const lower = [...view.querySelectorAll(".gx-chip")].find((b) => b.textContent === "Lower thirds");
  lower.click();
  test("a filter chip narrows the cards", () => eq(view.querySelectorAll(".gx-card").length, 1));
  view.remove();
  for (const t of document.querySelectorAll(".toast")) t.remove();
}
