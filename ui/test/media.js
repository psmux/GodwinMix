// The Media panel's folders and tags against a stubbed client: the model on
// its own, then the real panel drawn and driven the way a person does, with
// a mouse drag, a right click menu, the dialogs and Select.

import { tree, visible, counts, tagsInUse, typedTags, canMoveFolder, fileTarget, folderLabel } from "../panels/media/model.js";
import { mediaStub, items, FOLDERS } from "./media-stub.js";

const settle = (ms) => new Promise((r) => setTimeout(r, ms));
const DASHES = [String.fromCharCode(0x2013), String.fromCharCode(0x2014)];
const button = (root, text) => [...root.querySelectorAll("button")].find((b) => b.textContent.trim() === text);
const names = (view) => [...view.querySelectorAll(".mx-row")].map((r) => r.dataset.dragMedia);
const node = (view, path) => view.querySelector(`.mx-node[data-folder="${path}"]`);
const dialog = () => document.querySelector(".dialog");

function pointer(target, type, x, y) {
  target.dispatchEvent(new PointerEvent(type, { bubbles: true, clientX: x, clientY: y, button: 0, pointerType: "mouse", isPrimary: true }));
}

/** A real drag with the mouse: down on one thing, across, up on another. */
async function drag(from, to) {
  const a = from.getBoundingClientRect();
  const b = to.getBoundingClientRect();
  pointer(from, "pointerdown", a.left + 10, a.top + 5);
  pointer(window, "pointermove", a.left + 30, a.top + 20);
  pointer(window, "pointermove", b.left + 10, b.top + b.height / 2);
  pointer(window, "pointerup", b.left + 10, b.top + b.height / 2);
  await settle(30);
}

function rightClick(target) {
  const r = target.getBoundingClientRect();
  target.dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, cancelable: true, clientX: r.left + 5, clientY: r.top + 5 }));
}

export async function mediaTests(test, eq, ok) {
  const all = items();
  test("folders make a tree, each level by name", () => {
    const t = tree(["Sermons/2026", "Ads", "Sermons"]);
    eq(t.map((n) => n.path), ["Ads", "Sermons"]);
    eq(t[1].children.map((n) => n.name), ["2026"]);
  });
  test("a folder shows its own files, and its subfolders' with the switch", () => {
    eq(visible(all, { folder: "Sermons" }).map((i) => i.name), ["Sermons/sunday.mp4"]);
    eq(visible(all, { folder: "Sermons", deep: true }).length, 2);
    eq(visible(all, { folder: "" }).map((i) => i.name), ["welcome.mp4"]);
    eq(visible(all).length, 4, "no folder is every file");
  });
  test("tags and words filter without regard to case", () => {
    eq(visible(all, { tags: ["EASTER"] }).length, 2);
    eq(visible(all, { tags: ["easter", "youth"] }).map((i) => i.name), ["Sermons/2026/easter.mp4"]);
    eq(visible(all, { query: "choir pro" }).map((i) => i.name), ["Ads/promo.mp4"]);
  });
  test("counts are files right in a folder and anywhere under it", () => {
    const c = counts(all, FOLDERS);
    eq(c.get("Sermons"), { files: 1, total: 2 });
    eq(c.get("Sermons/2026"), { files: 1, total: 1 });
  });
  test("tags in use keep the first spelling and count every file", () => {
    eq(tagsInUse(all)[0], { name: "Easter", count: 2 });
    eq(typedTags(" easter, Youth ,EASTER,, "), ["easter", "Youth"]);
  });
  test("a folder never goes inside itself, and a drop where a thing already is does nothing", () => {
    ok(!canMoveFolder("Sermons", "Sermons/2026"), "into its own folder");
    ok(!canMoveFolder("Sermons", "Sermons"), "into itself");
    ok(!canMoveFolder("Sermons/2026", "Sermons"), "where it is");
    ok(canMoveFolder("Sermons/2026", ""), "to the top");
    eq(fileTarget("Ads/promo.mp4", "Ads"), null);
    eq(folderLabel("Sermons/2026"), "Sermons / 2026");
  });

  await import("../panels/media/panel.js");
  const client = mediaStub({ refuseOnce: true });
  const view = document.createElement("gmx-media-view");
  view.setClient(client);
  view.style.cssText = "display:block;position:fixed;top:0;left:0;width:900px;z-index:50;background:var(--bg)";
  document.body.appendChild(view);
  await settle(80);

  test("the rail shows All files and the top folders with their counts, closed", () => {
    eq(node(view, "").textContent.replace(/\s+/g, " ").trim(), "All files4");
    ok(node(view, "Sermons") && node(view, "Ads"), "top folders");
    ok(!node(view, "Sermons/2026"), "a closed folder hides its folders");
    eq(names(view).length, 4, "All files is every file");
  });
  node(view, "Sermons").querySelector(".mx-caret").click();
  await settle(10);
  test("the caret opens a folder", () => ok(node(view, "Sermons/2026"), "opened"));
  node(view, "Sermons").click();
  await settle(10);
  test("picking a folder shows its files, and the switch adds its subfolders", () => {
    eq(names(view), ["Sermons/sunday.mp4"]);
    const box = view.querySelector(".mx-bar input[type=checkbox]");
    box.click();
    eq(names(view), ["Sermons/sunday.mp4", "Sermons/2026/easter.mp4"]);
  });
  node(view, "").click();
  view.querySelector(".mx-search").value = "youth";
  view.querySelector(".mx-search").dispatchEvent(new Event("input"));
  test("the search finds a file by its tag", () => eq(names(view), ["Sermons/2026/easter.mp4"]));
  view.querySelector(".mx-search").value = "";
  view.querySelector(".mx-search").dispatchEvent(new Event("input"));
  button(view.querySelector('[data-drag-media="Ads/promo.mp4"]'), "Choir").click();
  test("a tag chip filters by its tag, and its chip in the bar takes it off", () => {
    eq(names(view), ["Ads/promo.mp4"]);
    button(view.querySelector(".mx-bar"), "Choir ×").click();
    eq(names(view).length, 4);
  });

  await drag(view.querySelector('[data-drag-media="welcome.mp4"]'), node(view, "Ads"));
  await settle(30);
  test("a file dragged onto a folder moves there, and asks once when it is on air", () => {
    const yes = dialog() && button(dialog(), "Move anyway");
    ok(yes, "the on air question");
    yes.click();
  });
  await settle(30);
  test("then moves it with force", () => eq(client.sent("media.move"), [{ names: ["welcome.mp4"], folder: "Ads" }, { names: ["welcome.mp4"], folder: "Ads", force: true }]));
  await drag(node(view, "Sermons"), node(view, "Sermons/2026"));
  test("a folder dragged into its own folder does nothing", () => eq(client.sent("media.folder.move").length, 0));
  await drag(node(view, "Sermons/2026"), node(view, ""));
  test("a folder dragged onto All files goes to the top", () => eq(client.sent("media.folder.move"), [{ name: "Sermons/2026", into: "" }]));

  rightClick(view.querySelector('[data-drag-media="Ads/promo.mp4"]'));
  test("a file's menu has Tags, Move to and Rename", () => {
    const menu = document.querySelector(".menu");
    ok(menu && button(menu, "Tags") && button(menu, "Move to") && button(menu, "Rename"), menu && menu.textContent);
    button(menu, "Tags").click();
  });
  await settle(80);
  const tagBox = dialog().querySelector("input[list]");
  tagBox.value = "Easter, ads";
  tagBox.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  test("the tag dialog suggests tags in use and adds what is typed", () => {
    ok([...dialog().querySelectorAll("option")].some((o) => o.value === "youth"), "suggestions");
    eq([...dialog().querySelectorAll(".mx-tag")].map((t) => t.firstChild.textContent), ["Choir", "Easter", "ads"]);
    button(dialog(), "Save").click();
  });
  await settle(30);
  test("Save sends the file's tags", () => eq(client.sent("media.tag"), [{ names: ["Ads/promo.mp4"], tags: ["Choir", "Easter", "ads"] }]));

  rightClick(node(view, "Ads"));
  button(document.querySelector(".menu"), "Remove folder").click();
  await settle(80);
  test("removing a folder asks where its files go and deletes nothing", () => {
    ok(dialog().textContent.includes("No file is deleted"), dialog().textContent);
    dialog().querySelector('input[value="top"]').click();
    button(dialog(), "Remove folder").click();
  });
  await settle(30);
  test("and sends the choice", () => eq(client.sent("media.folder.remove"), [{ name: "Ads", contents: "top" }]));

  button(view, "Select").click();
  for (const n of ["welcome.mp4", "Ads/promo.mp4"]) view.querySelector(`[data-drag-media="${n}"] input[type=checkbox]`).click();
  test("Select chooses several and says how many", () => ok(view.querySelector(".mx-bar").textContent.includes("2 chosen")));
  button(view.querySelector(".mx-bar"), "Move to").click();
  await settle(80);
  button(dialog(), "Sermons").click();
  button(dialog(), "Move here").click();
  await settle(30);
  test("Move to moves every chosen file to the folder picked", () => eq(client.sent("media.move").pop(), { names: ["welcome.mp4", "Ads/promo.mp4"], folder: "Sermons" }));
  test("no word the panel shows has a dash in it", () => ok(!DASHES.some((d) => view.textContent.includes(d))));
  view.remove();
  for (const t of document.querySelectorAll(".toast, .menu, .scrim")) t.remove();
}
