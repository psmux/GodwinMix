// The loader. It is the first page the window shows and, most of the time,
// the operator never sees it: it asks the shell which core was last used,
// tells the shell to make that connection, and sends the window to the page
// the core serves. It only draws the dialog when there is nothing remembered,
// when the connection failed, or when the operator asked for it from the menu
// (the shell navigates here with #connect on the URL).
//
// Everything that touches a socket, a process or a file happens in Rust. This
// page holds no token, opens no port, and knows no default address.

const invoke = window.__TAURI__.core.invoke;

const status = document.getElementById("status");
const form = document.getElementById("form");
const remote = document.getElementById("remote");
const address = document.getElementById("address");
const token = document.getElementById("token");
const error = document.getElementById("error");
const go = document.getElementById("go");

const mode = () => form.querySelector('input[name="mode"]:checked').value;

function say(text, working) {
  status.textContent = text;
  status.classList.toggle("working", Boolean(working));
}

function fail(message) {
  error.textContent = message;
  error.hidden = false;
  form.hidden = false;
  go.disabled = false;
  say("Not connected.");
}

function showRemoteFields() {
  remote.hidden = mode() !== "remote";
}

/** Ask the shell to connect, then hand the window to the core's own UI. */
async function connect(wanted) {
  go.disabled = true;
  error.hidden = true;
  say(wanted.mode === "local" ? "Starting the mixer on this computer." : `Reaching ${wanted.address}.`, true);
  try {
    const core = await invoke("connect_core", { wanted });
    say(`${core.label} ${core.version}. Opening.`, true);
    window.location.replace(core.url);
  } catch (e) {
    fail(String(e));
  }
}

form.addEventListener("change", showRemoteFields);
form.addEventListener("submit", (e) => {
  e.preventDefault();
  connect({ mode: mode(), address: address.value.trim(), token: token.value });
});

(async function start() {
  const saved = await invoke("saved_connection");
  if (saved.mode === "remote") {
    form.querySelector('input[value="remote"]').checked = true;
    address.value = saved.address || "";
    token.value = saved.token || "";
  }
  showRemoteFields();

  // #connect means the operator chose Connect from the menu or the tray, so
  // the dialog is the point. Otherwise a remembered connection is made at
  // once and the window never really stops at this page.
  if (location.hash.startsWith("#connect") || !saved.remembered) {
    form.hidden = false;
    say(saved.remembered ? "Connected to a mixer. Pick another, or connect again." : "No mixer chosen yet.");
    return;
  }
  connect({ mode: saved.mode, address: saved.address || "", token: saved.token || "" });
})();
