// Drive the real UI in a headless Chrome with real mouse events, over the
// DevTools protocol, with nothing to install: node 24 has the WebSocket.
//
//   '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' --headless=new \
//       --remote-debugging-port=9333 --user-data-dir=/tmp/gmx-chrome about:blank &
//   node -e 'import("./dev/drive.mjs").then(async ({attach}) => {
//     const p = await attach(9333, "http://127.0.0.1:8080/");
//     await p.click("button", "=Add sources");        // "=" means the exact text
//     console.log(await p.audit(), p.net, p.logs);     // broken pictures, HTTP errors, console
//     await p.shot("after.png"); process.exit(0); })'
//
// A click holds the button down for a moment, because a press and release in
// the same millisecond is not what a person does and the page treats it
// differently. That difference is what a scripted test once missed.
import { writeFileSync } from "node:fs";

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export async function attach(port, url) {
  let page;
  for (let i = 0; i < 100 && !page; i++) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
      page = list.find((t) => t.type === "page" && t.webSocketDebuggerUrl);
    } catch {}
    if (!page) await sleep(200);
  }
  if (!page) throw new Error(`no page on DevTools port ${port}`);
  const socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((res) => socket.addEventListener("open", res, { once: true }));
  let id = 0;
  const waiting = new Map();
  const logs = [];
  const net = []; // every response of 400 or more, and every request that failed
  socket.addEventListener("message", (ev) => {
    const m = JSON.parse(ev.data);
    if (m.id && waiting.has(m.id)) {
      waiting.get(m.id)(m);
      waiting.delete(m.id);
    } else if (m.method === "Network.responseReceived") {
      const r = m.params.response;
      if (r.status >= 400) net.push(`${r.status} ${r.url}`);
    } else if (m.method === "Network.loadingFailed") {
      if (!m.params.canceled) net.push(`FAILED ${m.params.errorText} (${m.params.type})`);
    } else if (m.method === "Runtime.consoleAPICalled") {
      logs.push(`[${m.params.type}] ` + m.params.args.map((a) => a.value ?? a.description ?? "").join(" "));
    } else if (m.method === "Runtime.exceptionThrown") {
      const d = m.params.exceptionDetails;
      logs.push("[exception] " + (d.exception?.description || d.text));
    }
  });
  const send = (method, params = {}) =>
    new Promise((res) => {
      waiting.set(++id, res);
      socket.send(JSON.stringify({ id, method, params }));
    });
  await send("Runtime.enable");
  await send("Page.enable");
  await send("Network.enable");
  await send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 900, deviceScaleFactor: 1, mobile: false });
  if (url) {
    await send("Page.navigate", { url });
    await sleep(2500);
  }

  async function evaluate(expression) {
    const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description || "eval failed");
    return r.result?.result?.value;
  }

  // The centre of the nth visible element matching a selector whose text matches.
  async function locate(selector, text, nth = 0) {
    return evaluate(`(() => {
      const want = ${JSON.stringify(text ?? null)};
      const all = [...document.querySelectorAll(${JSON.stringify(selector)})].filter((e) => {
        const r = e.getBoundingClientRect();
        if (!r.width || !r.height) return false;
        if (want === null) return true;
        const t = (e.textContent || "").trim();
        return want.startsWith("=") ? t === want.slice(1) : t.includes(want);
      });
      const e = all[${nth}];
      if (!e) return null;
      e.scrollIntoView({ block: "center" });
      const r = e.getBoundingClientRect();
      const x = r.x + r.width / 2, y = r.y + r.height / 2;
      const top = document.elementFromPoint(x, y);
      return { x, y, count: all.length, covered: !(top === e || e.contains(top) || top?.contains(e)), disabled: !!e.disabled, text: (e.textContent || "").trim().slice(0, 60) };
    })()`);
  }

  async function mouse(type, x, y, extra = {}) {
    await send("Input.dispatchMouseEvent", { type, x, y, button: "left", buttons: type === "mouseReleased" ? 0 : 1, clickCount: 1, ...extra });
  }

  async function click(selector, text, opts = {}) {
    const at = await locate(selector, text, opts.nth || 0);
    if (!at) throw new Error(`nothing to click: ${selector} ${text ?? ""}`);
    if (at.covered) console.log(`  note: ${selector} "${text}" is covered by something else`);
    await mouse("mouseMoved", at.x, at.y, { buttons: 0 });
    const n = opts.double ? 2 : 1;
    for (let c = 1; c <= n; c++) {
      await mouse("mousePressed", at.x, at.y, { clickCount: c });
      await sleep(120);
      await mouse("mouseReleased", at.x, at.y, { clickCount: c });
    }
    await sleep(opts.wait ?? 600);
    return at;
  }

  async function key(k, extra = {}) {
    const base = { key: k, code: extra.code || (k.length === 1 ? "Key" + k.toUpperCase() : k), windowsVirtualKeyCode: extra.vk, ...extra };
    await send("Input.dispatchKeyEvent", { type: "keyDown", ...base, text: k.length === 1 ? k : undefined });
    await send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
    await sleep(300);
  }

  /** Type into whatever has focus, the way a keyboard would. */
  async function type(text) {
    await send("Input.insertText", { text });
    await sleep(200);
  }

  /** Press at one point, move in steps, release at another. */
  async function drag(x1, y1, x2, y2) {
    await mouse("mouseMoved", x1, y1, { buttons: 0 });
    await mouse("mousePressed", x1, y1);
    for (let i = 1; i <= 8; i++) {
      await mouse("mouseMoved", x1 + ((x2 - x1) * i) / 8, y1 + ((y2 - y1) * i) / 8);
      await sleep(30);
    }
    await mouse("mouseReleased", x2, y2);
    await sleep(400);
  }

  async function shot(path) {
    const r = await send("Page.captureScreenshot", { format: "png" });
    writeFileSync(path, Buffer.from(r.result.data, "base64"));
  }

  /** Every picture that failed to draw, and every visible text that looks like a leak. */
  async function audit() {
    return evaluate(`(() => {
      const vis = (e) => { const r = e.getBoundingClientRect(); return r.width > 0 && r.height > 0; };
      const broken = [...document.images].filter((i) => vis(i) && i.complete && i.naturalWidth === 0).map((i) => i.currentSrc || i.src || "(no src)");
      const pending = [...document.images].filter((i) => vis(i) && !i.complete).map((i) => i.src);
      const media = [...document.querySelectorAll("canvas,video")].filter(vis).map((c) => c.tagName + " " + c.width + "x" + c.height);
      const leaks = [...document.querySelectorAll("body *")].filter((e) => vis(e) && e.children.length === 0 && /undefined|\\\\[object Object\\\\]|NaN|^null$/.test(e.textContent)).map((e) => e.textContent.trim().slice(0, 80));
      return { broken, pending, media, leaks };
    })()`);
  }

  return { send, evaluate, locate, click, key, shot, logs, net, mouse, type, drag, audit };
}
