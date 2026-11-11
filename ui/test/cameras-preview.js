// The camera window on its demo backend, for looking at and for
// screenshots: /test/cameras.html?theme=light, &pro=1 for the Pro view,
// &demo=0 for the window with no camera plugin answering, &follow=1 to
// have the centre camera follow someone. Nothing here talks to a core.

const q = new URLSearchParams(location.search);
const theme = q.get("theme") || "dark";
document.getElementById("gmx-theme").href = `../themes/${theme}.css`;
document.documentElement.dataset.theme = theme;
const wait = (ms) => new Promise((r) => setTimeout(r, ms));

/** A client with no mixer behind it: every call is refused, as an offline core would. */
const offline = {
  state: { connected: false, tally: {} },
  call: async (method) => { throw new Error(`${method}: no mixer is answering`); },
  on: () => () => {},
  onRender: () => () => {},
  listen: () => () => {},
};

async function main() {
  try {
    localStorage.setItem("gmx.cameras", JSON.stringify({ demo: q.get("demo") !== "0", pro: q.get("pro") === "1" }));
  } catch { /* fine */ }
  const { mount } = await import("../panels/cameras/mount.js");
  const host = document.getElementById("host");
  host.client = offline;
  const view = mount(host);
  window.__cams = view;
  await wait(1200);
  const client = view.client;
  if (q.get("follow") === "1" && client.demo) {
    const api = view.model.api;
    await api.framing("cam-centre", "waist");
    await api.followPerson("cam-centre", "person-1");
    await api.track("cam-side", true);
    await api.framing("cam-side", "full");
    view.model.pick("cam-centre");
  }
  await wait(Number(q.get("settle") || 2500));
  document.body.dataset.ready = "1";
}

main();
