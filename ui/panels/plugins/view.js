// Sources > More plugins, and More plugins in the add source picker: every
// plugin the mixer's plugin lists know, as cards, over the page and under the
// top bar. Nothing is fetched until it opens; then one `plugin.gallery`, and
// the pictures come straight from GitHub. On All with no search, a Featured
// row and a New and updated row come first, as an app store opens.

import { el, clear } from "../../shell/dom.js";
import { card } from "./card.js";
import { openDetail, closeDetail } from "./detail.js";
import { FILTERS, visible, freshness } from "./model.js";
import { featured, fresh, featureCard, row } from "./rows.js";
import { makeStore } from "./store.js";

let open = null;

/** Open the page, or close it when it is open. */
export function toggleGallery(client) {
  if (open) return open.close();
  open = openGallery(client);
  return open;
}

function sheet() {
  if (document.getElementById("gmx-plugins-css")) return;
  document.head.append(el("link#gmx-plugins-css", { rel: "stylesheet", href: new URL("./plugins.css", import.meta.url).href }));
  document.head.append(el("link#gmx-store-css", { rel: "stylesheet", href: new URL("./store.css", import.meta.url).href }));
}

export function openGallery(client, opts = {}) {
  sheet();
  const back = document.activeElement;
  const state = { gallery: null, filter: "all", loading: true, error: "" };
  const search = el("input.pg-search", { type: "search", placeholder: "Search plugins", "aria-label": "Search plugins" });
  const chips = el("div.pg-chips", { role: "toolbar", "aria-label": "Show only" });
  const status = el("span.pg-dim", { role: "status" });
  const rows = el("div.pg-rows");
  const grid = el("div.pg-grid");
  const root = el("section.plugins-page", { role: "region", "aria-label": "Plugins" }, [
    el("header.pg-top", {}, [
      el("h2", { text: "Plugins" }), status, el("span.grow"), search,
      el("button.btn", { type: "button", text: "Check again", title: "Fetch the plugin list again", onclick: () => load(true) }),
      el("button.btn.icon", { type: "button", text: "×", title: "Close plugins (Escape)", "aria-label": "Close plugins", onclick: () => view.close() }),
    ]),
    chips,
    el("div.pg-scroll", {}, [rows, el("h3.pg-all", { text: "All plugins" }), grid]),
  ]);
  const header = document.querySelector(".slot-header, gmx-header");
  root.style.top = `${header ? Math.round(header.getBoundingClientRect().bottom) : 44}px`;
  document.body.append(root);
  // A show tab closes this page, since it covers the show's own page.
  root.dataset.cover = "";
  root.leave = () => view.close();
  const store = makeStore(client, { reload: () => load(false), leave: () => view.close(), arrive: opts.arrive });
  const detail = (p) => openDetail(client, p, () => load(false), store);

  function draw() {
    clear(chips);
    for (const f of FILTERS) {
      chips.append(el("button.btn.pg-chip", {
        type: "button", text: f.label, "aria-pressed": String(state.filter === f.id),
        onclick: () => { state.filter = f.id; draw(); },
      }));
    }
    const all = (state.gallery && state.gallery.plugins) || [];
    status.textContent = state.loading && !state.gallery ? "Getting the list." : state.error ? "" : freshness(state.gallery);
    const shown = visible(all, state.filter, search.value);
    drawRows(all);
    clear(grid);
    for (const p of shown) {
      const node = card(p, detail, store.button);
      const note = store.note(p.name);
      if (note) node.append(note);
      grid.append(node);
    }
    if (!shown.length) grid.append(el("p.pg-empty", { text: empty(all) }));
  }

  function drawRows(all) {
    clear(rows);
    const browsing = state.filter === "all" && !search.value.trim();
    root.classList.toggle("browsing", browsing);
    if (!browsing) return;
    const top = featured(all);
    if (top.length) rows.append(row("Featured", top.map((p) => featureCard(p, detail, (q) => store.button(q, ".pg-big")))));
    const recent = fresh(all);
    if (recent.length) rows.append(row("New and updated", recent.map((p) => card(p, detail, store.button))));
  }

  function empty(all) {
    if (state.error) return `The plugin list did not load: ${state.error}`;
    if (state.loading) return "Getting the list of plugins.";
    if (!all.length) return freshness(state.gallery);
    return search.value ? "No plugin matches that search." : "None of these yet.";
  }

  async function load(force) {
    state.loading = true;
    if (!state.gallery) draw();
    try {
      state.gallery = await client.call("plugin.gallery", force ? { refresh: true } : {});
      state.error = "";
    } catch (e) {
      state.error = (e && e.message) || String(e);
    }
    state.loading = false;
    draw();
    return (state.gallery && state.gallery.plugins) || [];
  }

  search.addEventListener("input", draw);
  const onKey = (e) => {
    if (e.key !== "Escape" || document.querySelector(".scrim")) return;
    if (e.target === search && search.value) return;
    view.close();
  };
  window.addEventListener("keydown", onKey);
  const view = {
    root,
    store,
    reload: load,
    close() {
      window.removeEventListener("keydown", onKey);
      closeDetail();
      root.remove();
      open = null;
      back?.focus?.();
    },
  };
  load(false);
  search.focus();
  return view;
}
