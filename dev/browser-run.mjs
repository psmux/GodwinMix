// Open a page in a running headless Chrome, wait for it to finish, print it.
//
// Used by dev/ui-tests.sh to read the DOM harness. Chrome's `--dump-dom`
// cannot be used for this: it prints the page as soon as it loads, and the
// harness has real work to do afterwards against a real core, and
// `--virtual-time-budget` is worse than useless here because it races the
// page's own waits past a WebSocket that is still connecting.
//
// So this talks the DevTools protocol instead, over the WebSocket that node 24
// has built in. No dependency, no install, about sixty lines.
//
//   node dev/browser-run.mjs <debugging port> <url> [<seconds>]
//
// It exits 0 when the page says it passed, 1 when it says it failed or never
// finished. The page says so by putting "pass" or "fail" in `#out`'s
// `data-result`, which is what ui/test/run.js already does.

const [port, url, seconds = "180"] = process.argv.slice(2);
if (!port || !url) {
  console.error("usage: node dev/browser-run.mjs <port> <url> [seconds]");
  process.exit(2);
}

const deadline = Date.now() + Number(seconds) * 1000;

/** Chrome's list of targets, once it is listening. */
async function target() {
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`http://127.0.0.1:${port}/json/list`);
      const list = await res.json();
      const page = list.find((t) => t.type === "page" && t.webSocketDebuggerUrl);
      if (page) return page;
    } catch {
      /* Chrome is still starting */
    }
    await sleep(200);
  }
  throw new Error(`Chrome never listened on ${port}`);
}

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

const page = await target();
const socket = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.addEventListener("open", resolve, { once: true });
  socket.addEventListener("error", () => reject(new Error("could not attach to the page")), { once: true });
});

let id = 0;
const waiting = new Map();
socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  const pending = waiting.get(message.id);
  if (!pending) return;
  waiting.delete(message.id);
  pending(message);
});

function send(method, params) {
  return new Promise((resolve) => {
    const mine = ++id;
    waiting.set(mine, resolve);
    socket.send(JSON.stringify({ id: mine, method, params: params || {} }));
  });
}

/** One expression in the page, as a plain value. */
async function evaluate(expression) {
  const answer = await send("Runtime.evaluate", { expression, returnByValue: true });
  return answer.result && answer.result.result ? answer.result.result.value : undefined;
}

await send("Page.enable");
await send("Runtime.enable");
await send("Page.navigate", { url });

// Console errors are the first thing anybody wants when a page fails, so they
// are printed as they happen rather than hunted for afterwards.
socket.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.method !== "Runtime.consoleAPICalled") return;
  if (message.params.type !== "error") return;
  const text = (message.params.args || []).map((a) => a.value || a.description || "").join(" ");
  if (text) console.error("  console: " + text.split("\n")[0]);
});

let result = "";
while (Date.now() < deadline) {
  result = (await evaluate("(document.getElementById('out')||{}).dataset && document.getElementById('out').dataset.result || ''")) || "";
  if (result) break;
  await sleep(500);
}

const text = (await evaluate("(document.getElementById('out')||{}).innerText || ''")) || "";
console.log(text.trimEnd());
socket.close();

if (!result) {
  console.error(`\nthe page never finished within ${seconds} seconds`);
  process.exit(1);
}
process.exit(result === "pass" ? 0 : 1);
