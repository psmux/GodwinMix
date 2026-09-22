// Boot: ask for a token if the mixer wants one, connect, register the first
// party panels, mount the shell.
//
// No inline script anywhere in this page, so the Content Security Policy the
// server sets can be a real one rather than a hole with `unsafe-inline` in it.

import { connect, storedToken, storeToken, migrateLegacyKeys } from "./client/index.js";
import { installGlobal } from "./shell/registry.js";
import { mountShell } from "./shell/shell.js";
import { askForToken } from "./shell/firstrun.js";
import { initTheme } from "./shell/theme.js";
import { toast, confirmHook } from "./shell/toast.js";
import { proposeGalleryMode } from "./shell/settings.js";
import { meterClient } from "./shell/meter.js";
import { applyCoreDefaults, watchCoreDefaults } from "./panels/welcome/defaults.js";

const PANELS = [
  "./panels/header/panel.js",
  "./panels/multiview/panel.js",
  "./panels/sources/panel.js",
  "./panels/scenes/panel.js",
  "./panels/outputs/panel.js",
  "./panels/audio/panel.js",
  "./panels/media/panel.js",
  "./panels/alerts/panel.js",
  "./panels/welcome/panel.js",
];

/**
 * Does this mixer want a token, and do we have the right one?
 *
 * One request, before the socket, because a 401 on a WebSocket is a silent
 * close and a 401 on a fetch is a number we can act on. The old page learned
 * this the hard way.
 */
async function authorise(base) {
  let token = storedToken();
  for (let attempt = 0; attempt < 3; attempt += 1) {
    let status;
    try {
      const res = await fetch(new URL("/api/status", base), {
        headers: token ? { Authorization: "Bearer " + token } : {},
      });
      status = res.status;
    } catch {
      // The mixer is not answering at all. Connecting anyway gives the client's
      // own retry loop a chance, and the banner says what is happening.
      return token;
    }
    if (status !== 401) return token;
    // A saved token that is refused is worse than none: it fails silently on
    // every reload. Drop it and ask.
    if (attempt > 0) storeToken(null);
    const answer = await askForToken(
      attempt === 0
        ? null
        : "That token was refused. Check it with whoever runs this mixer. If you are an admin here and the token is lost, set a new one in Mixer settings under Control token from a page that is already connected."
    );
    if (!answer) return null;
    token = answer;
  }
  return token;
}

async function main() {
  migrateLegacyKeys();
  initTheme();

  // What the gallery would start as if no preset says otherwise. Read before
  // the client exists so the toast below can explain a modest machine; the
  // core's own answer, when a preset applied one, replaces it a moment later.
  const proposed = proposeGalleryMode();

  const token = await authorise(location.origin);
  const client = await connect({ token });
  window.gmxClient = client;
  // A token set to confirm destructive calls asks the person here, and the
  // call goes again with the confirmation when they say yes.
  client.confirm = confirmHook;
  // Who the meters ask for levels. Nothing is asked for until a panel puts a
  // meter on screen, and the ask is given back when the last one goes.
  meterClient(client);

  // The core's [ui] section: the theme, the gallery mode and the layout a
  // preset chose. Applied before the panels mount so nothing flashes.
  await applyCoreDefaults(client);
  watchCoreDefaults(client);

  installGlobal();
  await Promise.all(PANELS.map((path) => import(path).catch((e) => console.error(`panel ${path} failed`, e))));

  await mountShell(client);

  if (client.legacy) {
    console.info("this mixer has no /rpc yet, so the page is talking to the REST API through the legacy adapter");
  }
  if (proposed === "icon") {
    toast({
      text: "This machine looks modest, so tiles start as icons. Settings turns the pictures on.",
      ms: 12000,
    });
  }
}

main().catch((e) => {
  console.error(e);
  document.body.appendChild(
    Object.assign(document.createElement("pre"), {
      textContent: `The page could not start: ${e && e.message ? e.message : e}`,
      style: "padding:16px;color:#d9584c",
    })
  );
});
