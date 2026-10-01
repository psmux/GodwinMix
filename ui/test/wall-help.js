// What the wall's tests share: waiting, the calls a stub was sent, a key
// press, the open dialogs, and the wall opened on a stub with its first
// numbers read.

export const wait = (ms = 40) => new Promise((r) => setTimeout(r, ms));
export const until = async (fn, ms = 3000) => { const end = Date.now() + ms; while (!fn() && Date.now() < end) await wait(20); return fn(); };
export const sent = (stub, method) => stub.calls.filter((c) => c.method === method);
export const key = (node, k) => node.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true }));
export const dialogs = () => [...document.querySelectorAll(".dialog")];
export const button = (root, text) => [...root.querySelectorAll("button")].find((b) => b.textContent.trim() === text);
export const closeDialogs = () => { for (const d of dialogs()) d.closest(".scrim")?.remove(); };

export async function openOn(stub) {
  try { localStorage.removeItem("gmx.wall"); } catch { /* fine */ }
  const { toggleWall } = await import("../panels/wall/view.js");
  const view = toggleWall(stub);
  await until(() => view.root.querySelector(".wl-row"));
  await until(() => stub.statsAsked.length);
  await wait(80);
  return view;
}
