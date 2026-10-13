// The Plugins page against a stubbed core: the words on its own, then the
// page drawn, filtered, searched, and driven through one click Install, a
// failed install, Update and Remove in the detail view.

import { visible, count, primary, tier, oneLine, freshness, kindWords } from "../panels/plugins/model.js";
import { facts } from "../panels/plugins/detail.js";
import { plainly } from "../panels/plugins/act.js";
import { openGallery } from "../panels/plugins/view.js";
import { GalleryStub, plugins } from "./plugins-stub.js";

const settle = (ms = 30) => new Promise((r) => setTimeout(r, ms));
const DASHES = [String.fromCharCode(0x2013), String.fromCharCode(0x2014)];

async function until(fn, ms = 3000) {
  for (let t = 0; t < ms; t += 20) {
    if (fn()) return true;
    await settle(20);
  }
  return false;
}

const cardOf = (view, name) => view.root.querySelector(`.pg-card[data-name="${name}"]`);
const chip = (view, label) => [...view.root.querySelectorAll(".pg-chip")].find((b) => b.textContent === label);

export async function pluginGalleryTests(test, eq, ok) {
  const all = plugins();
  test("plugins: a chip and a search keep the cards a person means", () => {
    eq(visible(all, "all", "").length, 4);
    eq(visible(all, "source", "").map((p) => p.name), ["camera", "clock", "ndi"]);
    eq(visible(all, "filter", "").map((p) => p.name), ["macfx"]);
    eq(visible(all, "output", "").map((p) => p.name), ["ndi"]);
    eq(visible(all, "installed", "").map((p) => p.name), ["ndi"]);
    eq(visible(all, "all", "TIME").map((p) => p.name), ["clock"], "tags are searched, any case");
    eq(visible(all, "all", "finds devices").map((p) => p.name), ["camera"], "and the kind words");
  });

  test("plugins: counts, tiers and one line read plainly", () => {
    eq([count(8), count(1240), count(98000), count(2500000), count(undefined)], ["8", "1.2k", "98k", "2.5M", ""]);
    eq(tier({ tier: "gold" }).label, "Gold");
    eq(tier({}).label, "Unreviewed");
    eq(oneLine(all[0].description), "A USB or built in camera as a source: v4l2 on Linux.");
    eq(kindWords(all[0]), ["Input", "Finds devices"]);
  });

  test("plugins: the first button installs, updates, says installed, or says why not", () => {
    eq(primary(all[1]), { label: "Install", act: "add" });
    eq(primary(all[3]), { label: "Update", act: "update" });
    eq(primary({ ...all[3], update: false }), { label: "Installed", act: null });
    eq(primary(all[2]), { label: "Not for Windows", act: null });
  });

  test("plugins: the list says how fresh it is, and says offline when it is", () => {
    const now = 1_000_000_000_000;
    eq(freshness({ marketplaces: [{ fetched: now / 1000 - 120 }] }, now), "Updated 2 min ago.");
    eq(freshness({ offline: true, marketplaces: [{ fetched: now / 1000 - 7200 }] }, now), "Offline. Showing the list from 2 h ago.");
    eq(freshness({ marketplaces: [{ fetched: 0 }] }, now).startsWith("The plugin list could not be fetched"), true);
    eq(freshness({ marketplaces: [] }, now), "No plugin list is set up on this mixer.");
  });

  test("plugins: the detail facts leave out what the listing does not say", () => {
    const rows = Object.fromEntries(facts(all[3]));
    eq(rows.Version, "1.0.0 (you have 0.9.0)");
    eq(rows["Older versions"], "0.9.0");
    ok(!("Signed" in Object.fromEntries(facts(all[0]))), "an included plugin is not asked about signing");
    eq(plainly("add", new Error("`clock` has no asset for windows-x86_64. It ships: linux.")), "It did not install: clock has no asset for windows-x86_64.");
  });

  // The page itself.
  const stub = new GalleryStub({ fail: { macfx: "the download stopped halfway." } });
  const view = openGallery(stub);
  await until(() => view.root.querySelectorAll(".pg-card").length === 4);
  const text = view.root.textContent;
  test("plugins: the page draws a card for each plugin with its stars, downloads and fit", () => {
    eq(view.root.querySelectorAll(".pg-card").length, 4);
    const clock = cardOf(view, "clock");
    ok(clock.textContent.includes("1.2k") && clock.textContent.includes("98k"), clock.textContent);
    ok(cardOf(view, "camera").textContent.includes("Included"));
    ok(cardOf(view, "macfx").textContent.includes("Not for Windows"));
    ok(cardOf(view, "macfx").querySelector(".pg-foot button").disabled, "nothing to press for a plugin that cannot run");
    ok(cardOf(view, "camera").querySelector(".pg-stand"), "no thumbnail: the kind's icon stands in");
    ok(!DASHES.some((d) => text.includes(d)), "no dashes in the words shown");
  });

  chip(view, "Outputs").click();
  test("plugins: a chip narrows the grid", () => {
    eq([...view.root.querySelectorAll(".pg-card")].map((c) => c.dataset.name), ["ndi"]);
  });
  chip(view, "All").click();

  cardOf(view, "clock").querySelector(".pg-foot button").click();
  await until(() => cardOf(view, "clock")?.textContent.includes("Installed. Nothing restarted."));
  test("plugins: one click installs by name and the card then says installed", () => {
    ok(stub.calls.some(([m, p]) => m === "plugin.add" && p.source === "clock"), JSON.stringify(stub.calls));
    ok(cardOf(view, "clock").textContent.includes("Installed. Nothing restarted."));
    eq(cardOf(view, "clock").querySelector(".pg-foot button").textContent, "Installed");
  });

  stub.list.find((p) => p.name === "macfx").runs_here = true;
  await view.reload();
  cardOf(view, "macfx").querySelector(".pg-foot button").click();
  await until(() => cardOf(view, "macfx")?.querySelector(".pg-note.bad"));
  test("plugins: a failed install says why in plain words on the card", () => {
    eq(cardOf(view, "macfx").querySelector(".pg-note.bad")?.textContent, "It did not install: the download stopped halfway.");
  });

  cardOf(view, "ndi").querySelector(".pg-open").click();
  await settle();
  const dialog = document.querySelector(".dialog");
  const button = (label) => [...document.querySelectorAll(".dialog footer button")].find((b) => b.textContent === label);
  test("plugins: the detail view has the facts, the source link and Update and Remove", () => {
    ok(dialog && dialog.textContent.includes("NDI in and out."));
    ok(button("Update") && button("Remove"), "both buttons");
    const box = dialog.getBoundingClientRect();
    ok(dialog.contains(document.elementFromPoint(box.x + box.width / 2, box.y + 12)), "the detail opens over the page, not under it");
  });
  button("Update")?.click();
  await until(() => document.querySelector(".dialog .pg-note")?.textContent === "Updated.");
  test("plugins: Update runs plugin.update and the view opens again without the button", () => {
    ok(stub.calls.some(([m, p]) => m === "plugin.update" && p.id === "ndi"));
    ok(!button("Update"), "nothing left to update");
    eq(document.querySelector(".dialog .pg-note")?.textContent, "Updated.");
  });
  button("Remove")?.click();
  await settle();
  [...document.querySelectorAll(".dialog footer button")].find((b) => b.textContent === "Remove" && b.classList.contains("primary"))?.click();
  await until(() => stub.calls.some(([m]) => m === "plugin.remove"));
  await settle(100);
  test("plugins: Remove asks first, then calls plugin.remove", () => {
    ok(stub.calls.some(([m, p]) => m === "plugin.remove" && p.id === "ndi"));
  });
  for (const b of document.querySelectorAll(".dialog header button")) b.click();
  view.close();

  const offline = openGallery(new GalleryStub({ offline: true }));
  await until(() => offline.root.querySelectorAll(".pg-card").length === 4);
  test("plugins: offline, the page shows the last list and says so", () => {
    ok(offline.root.querySelector(".pg-top").textContent.includes("Offline. Showing the list from 2 h ago."));
  });
  offline.close();
}
