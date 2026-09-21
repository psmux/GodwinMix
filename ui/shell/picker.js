// The add source and add output picker.
//
// OBS shows a text list of source types, and so did this: four tiles, every
// one of them a box to type a URI into. Somebody with a camera plugged in had
// nothing to click.
//
// So the source picker is a rail of categories with the content beside it, in
// the spirit of Wirecast's Add Shot and vMix's Add Input. Cameras, screens and
// microphones come first and they list real hardware, one row per device, from
// `device.discover`. The address boxes are still here, under the category they
// belong to, for the feed that has to be typed. A category whose plugin is
// missing still appears, with a button that installs it.
//
// The output picker is the older tile grid, unchanged: an output is an address
// and a key, there is no hardware to find, and a rail of one category would be
// a rail for its own sake.

import { el, clear, svg, on, fmtBytes } from "./dom.js";
import { sourceFiles } from "./source-files.js";
import { modal } from "./modal.js";
import { toast, errorToast } from "./toast.js";
import {
  ICONS,
  KIND_COLOUR,
  SOURCE_KINDS,
  OUTPUT_KINDS,
  CATEGORIES,
  TEST_PATTERNS,
  loadKinds,
  listPlugins,
  hasPlugin,
  grouped,
  kindOfUri,
  categoryOf,
  categoryOfProvide,
  discoverDevices,
  candidateSize,
  addRequestFor,
  alreadyAdded,
  sameAddress,
  withDeviceChoices,
  pluginSourceFor,
} from "../client/kinds.js";

const LIST_KEY = "gmx.picker.list";

/**
 * @param {object} client
 * @param {"source"|"output"} what
 * @param {{preset?: object, kind?: string, category?: string, scene?: string,
 *          onAdded?: (answer: object) => any}}
 *        opts  a dropped file or URL arrives here as `{preset: {uri}, kind}`,
 *        which skips straight to the form. `category` opens the rail on one
 *        category, which is what the empty tray does. `onAdded` is the
 *        caller's follow up on the thing that was just made, which is how the
 *        tray and the scene tabs put a new source in a scene.
 */
export async function openPicker(client, what, opts = {}) {
  const plugins = await listPlugins(client);
  const kinds = await loadKinds(client, what, plugins).catch(
    () => (what === "output" ? OUTPUT_KINDS : SOURCE_KINDS)
  );
  if (opts.kind) {
    const chosen = kinds.find((k) => k.id === opts.kind);
    if (chosen) return openForm(client, what, chosen, opts.preset, opts);
  }
  if (what === "output") return openTiles(client, what, kinds, opts);
  return openSourcePicker(client, kinds, plugins, opts);
}

// ------------------------------------------------------------ the source picker

function openSourcePicker(client, kinds, plugins, opts) {
  const categories = opts.existing ? [{ id: "existing", title: "Existing sources", icon: "sources" }, ...CATEGORIES] : CATEGORIES;
  let closed = false;
  const state = {
    category: categories.some((c) => c.id === opts.category) ? opts.category : CATEGORIES[0].id,
    plugins,
    // Discovery and the media listing both fill in after the modal is up. Each
    // one is idle, looking, done or failed, and the panel says which.
    devices: "idle",
    candidates: [],
    deviceError: "",
    media: "idle",
    items: [],
    mediaError: "",
    // What this picker has added in its own lifetime. The source list arrives
    // over the event stream a moment later, and a row that still said Add the
    // instant after it was pressed reads as a button that did nothing.
    added: new Set(),
    created: new Map(),
  };

  const files = sourceFiles(client, { onAdded: opts.onAdded });
  const search = el("input", {
    type: "search",
    placeholder: "Search everything",
    "aria-label": "Search everything that can be added",
  });
  const rail = el("nav.picker-rail", { role: "tablist", "aria-label": "Categories" });
  const panel = el("div.picker-panel");
  const body = el("div.picker", {}, [
    rail,
    el("div.picker-main", {}, [el("div.picker-head", {}, [search]), panel]),
  ]);

  const tabs = new Map();
  for (const cat of categories) {
    const tab = el(
      "button",
      {
        type: "button",
        role: "tab",
        onclick: () => {
          search.value = "";
          state.category = cat.id;
          draw();
        },
      },
      [icon(cat.icon), el("span.grow", { text: cat.title })]
    );
    tabs.set(cat.id, tab);
    rail.appendChild(tab);
  }

  const m = modal({
    title: opts.title || "Add a source",
    body,
    onClose: () => { closed = true; files.destroy(); opts.existing?.deactivate(); opts.onClose?.(); },
    footer: [el("button.btn", { text: "Close", onclick: () => m.close() })],
    wide: true,
  });

  on(search, "input", draw);
  draw();
  search.focus();
  rescan();
  loadMedia();
  return m;

  // ---------------------------------------------------------------- drawing

  function draw() {
    if (closed) return;
    const query = search.value.trim().toLowerCase();
    if (!query && state.category !== "existing") opts.existing?.deactivate();
    for (const [id, tab] of tabs) {
      const active = !query && id === state.category;
      tab.classList.toggle("on", active);
      tab.setAttribute("aria-selected", active ? "true" : "false");
    }
    clear(panel);
    if (query) return drawSearch(query);
    drawCategory(categories.find((c) => c.id === state.category) || CATEGORIES[0]);
  }

  /** One category, with its own heading and whatever it has to offer. */
  function drawCategory(cat) {
    if (cat.id === "existing") {
      opts.existing.draw("");
      panel.append(opts.existing.node);
      return;
    }
    const head = el("div.row", {}, [el("strong.grow", { text: cat.title })]);
    if (cat.devices) {
      head.appendChild(
        el("button.btn", { text: "Rescan", onclick: () => rescan(), disabled: state.devices === "looking" })
      );
    }
    if (cat.media) head.appendChild(el("button.btn", { text: "Rescan", onclick: () => loadMedia() }));
    panel.appendChild(head);
    if (cat.media) panel.append(files.node);

    if (cat.plugin && !hasPlugin(state.plugins, cat.plugin.name)) {
      panel.appendChild(installBlock(cat));
      return;
    }

    const rows = rowsFor(cat, "");
    for (const entry of rows) panel.appendChild(row(entry));
    if (!rows.length && waitingLine(cat)) panel.appendChild(el("p.dim", { text: waitingLine(cat) }));
    for (const entry of extraRowsFor(cat, "")) panel.appendChild(row(entry));

    const tiles = kindsFor(cat, "");
    if (!tiles.length) return;
    if (rows.length) panel.appendChild(el("div.group-title", { text: "Or set one up by hand" }));
    panel.appendChild(tileGrid(tiles));
  }

  /** Every category at once, keeping only what the typing matches. */
  function drawSearch(query) {
    let found = 0;
    if (opts.existing) {
      opts.existing.draw(query);
      panel.append(el("div.group-title", { text: "Existing sources" }), opts.existing.node);
      found++;
    }
    for (const cat of CATEGORIES) {
      const rows = rowsFor(cat, query).concat(extraRowsFor(cat, query));
      const tiles = kindsFor(cat, query);
      if (!rows.length && !tiles.length) continue;
      found += rows.length + tiles.length;
      panel.appendChild(el("div.group-title", { text: cat.title }));
      for (const entry of rows) panel.appendChild(row(entry));
      if (tiles.length) panel.appendChild(tileGrid(tiles));
    }
    if (!found) panel.appendChild(el("p.dim", { text: `Nothing matches "${search.value}".` }));
  }

  /** What a category says while it has nothing to list yet. */
  function waitingLine(cat) {
    if (cat.devices) {
      if (state.devices === "looking") return "Looking for devices.";
      if (state.devices === "failed") return state.deviceError || "The mixer could not look for devices.";
      return cat.nothing || "Nothing was found.";
    }
    if (cat.media) {
      if (state.media === "looking") return "Reading the media library.";
      if (state.media === "failed") return state.mediaError || "The media library could not be read.";
      return "Nothing in the media library yet. Browse files or drop a file on the window to upload one.";
    }
    return "";
  }

  // ---------------------------------------------------------------- entries

  /**
   * The rows of one category: found devices, library clips, test patterns.
   *
   * Every one of them is the same shape, so the renderer below is one function
   * whether it is drawing a camera or a colour bar.
   */
  function rowsFor(cat, query) {
    let rows = [];
    if (cat.plugin && !hasPlugin(state.plugins, cat.plugin.name)) return [];
    if (cat.devices) {
      rows = state.candidates
        .filter((c) => categoryOfProvide(c.type || c.kind) === cat.id)
        .map((candidate) => deviceEntry(cat, candidate));
    } else if (cat.media) {
      rows = state.items.map((item) => mediaEntry(item));
    } else if (cat.patterns) {
      rows = TEST_PATTERNS.map((pattern) => patternEntry(pattern));
    }
    return matching(rows, query);
  }

  /** Rows that are not a listing: the file the library has never seen. */
  function extraRowsFor(cat, query) {
    if (!cat.media || state.media === "looking") return [];
    return matching([browseEntry()], query);
  }

  function matching(rows, query) {
    if (!query) return rows;
    return rows.filter((e) => `${e.name} ${e.note || ""}`.toLowerCase().includes(query));
  }

  function deviceEntry(cat, candidate) {
    const params = addRequestFor(candidate);
    const size = candidateSize(candidate);
    return {
      icon: cat.icon,
      name: candidate.name || params.type,
      note: size || params.type,
      title: params.type,
      params: () => params,
      existing: () => sources().find(source => alreadyAdded([source], candidate)),
      added: () => state.added.has(key(candidate.name)) || alreadyAdded(sources(), candidate),
    };
  }

  function mediaEntry(item) {
    const uri = item.converted_path || item.path;
    return {
      icon: "media",
      name: item.name,
      note: item.size_bytes ? fmtBytes(item.size_bytes) : item.path,
      title: item.path,
      params: () => ({ uri, name: item.name }),
      existing: () => sources().find(source => sameAddress(source.uri, uri)),
      added: () => state.added.has(key(item.name)) || sources().some((s) => sameAddress(s.uri, uri)),
    };
  }

  /**
   * The file that is not in the library.
   *
   * Browse files uploads from the browser. This separate path form is for a
   * file that is already present on the mixer and needs no upload.
   */
  function browseEntry() {
    const kind = kinds.find((k) => k.id === "file") || kinds[0];
    return {
      icon: "file",
      name: "A file already on the mixer",
      note: "A path on the machine the mixer runs on",
      open: kind || null,
      label: "Enter path",
      added: () => false,
    };
  }

  function patternEntry(pattern) {
    return {
      icon: "pattern",
      name: pattern.name,
      note: pattern.note,
      title: pattern.uri,
      params: () => ({ uri: pattern.uri, name: pattern.name }),
      existing: () => sources().find(source => sameAddress(source.uri, pattern.uri)),
      added: () => state.added.has(key(pattern.name)) || sources().some((s) => sameAddress(s.uri, pattern.uri)),
    };
  }

  /** The kinds that belong to a category, which are the ones with a form. */
  function kindsFor(cat, query) {
    const mine = kinds.filter((k) => categoryOf(k) === cat.id);
    if (!query) return mine;
    return mine.filter(
      (k) =>
        k.title.toLowerCase().includes(query) ||
        String(k.description).toLowerCase().includes(query) ||
        k.id.toLowerCase().includes(query) ||
        String(k.plugin).toLowerCase().includes(query)
    );
  }

  // ---------------------------------------------------------------- pieces

  function row(entry) {
    const existing = entry.existing?.() || state.created.get(key(entry.name));
    const reusable = existing && opts.onExisting;
    const done = reusable ? opts.contains?.(existing) || state.added.has(key(entry.name)) : entry.added();
    const button = el("button.btn.primary", {
      text: done ? (opts.onExisting ? "In scene" : "Added") : entry.label || "Add",
      disabled: done,
    });
    button.onclick = async () => {
      if (entry.open) {
        m.close();
        openForm(client, "source", entry.open, opts.preset, opts);
        return;
      }
      button.disabled = true;
      button.textContent = "Adding";
      let answer;
      try {
        answer = reusable ? existing : await client.call("source.add", entry.params());
        state.created.set(key(entry.name), answer);
        if (reusable) await opts.onExisting(answer);
        else if (opts.onAdded) await opts.onAdded(answer);
        state.added.add(key(entry.name));
        toast({ text: `${entry.name} added.` });
        draw();
      } catch (e) {
        errorToast(e, answer ? `Source available, but could not add ${entry.name} to the scene` : entry.name);
        draw();
      }
    };
    return el("div.picker-row", { title: entry.title || "" }, [
      icon(entry.icon),
      el("span.grow", {}, [
        el("div.ellipsis", { text: entry.name }),
        entry.note ? el("div.dim.sm.ellipsis", { text: entry.note }) : null,
      ]),
      button,
    ]);
  }

  function tileGrid(list) {
    const grid = el("div.picker-grid");
    for (const kind of list) grid.appendChild(tile(client, "source", kind, opts, () => m.close()));
    return grid;
  }

  /**
   * A category whose plugin this mixer has not got.
   *
   * One sentence and one button. The button installs the plugin over the
   * protocol, the same call `gmx plugin add` makes, because telling an
   * operator to open a terminal is telling them the answer is somewhere else.
   */
  function installBlock(cat) {
    const note = el("p.sm.dim", { text: "" });
    const button = el("button.btn.primary", { text: cat.plugin.label });
    button.onclick = async () => {
      button.disabled = true;
      note.textContent = "Installing. It is fetched, checked and started; this can take a minute.";
      try {
        const source = await pluginSourceFor(client, cat.plugin.name);
        await client.call("plugin.add", { source });
      } catch (e) {
        errorToast(e, cat.plugin.label);
        button.disabled = false;
        note.textContent = "";
        return;
      }
      note.textContent = "Installed. Looking for devices.";
      state.plugins = await listPlugins(client);
      if (!hasPlugin(state.plugins, cat.plugin.name)) {
        // The install answered, so something is on disk, but the core has not
        // registered it. Saying so beats redrawing the same offer with no
        // word about what just happened.
        toast({
          kind: "warning",
          text: `${cat.plugin.name} was installed, but the mixer has not picked it up yet.`,
        });
      }
      await rescan();
    };
    return el("div.col", {}, [
      el("p.dim", { text: cat.plugin.line, style: { marginTop: "0" } }),
      // In a row rather than loose in the column, which would stretch a
      // primary button the whole width of the modal.
      el("div.row", {}, [button, note, el("span.grow")]),
    ]);
  }

  function icon(name) {
    const node = svg(ICONS[name] || ICONS.device, 20);
    node.style.color = KIND_COLOUR[name] || "var(--accent)";
    return node;
  }

  // ---------------------------------------------------------------- loading

  function sources() {
    return (client.state && client.state.sources) || [];
  }

  function key(name) {
    return String(name || "").trim().toLowerCase();
  }

  async function rescan() {
    state.devices = "looking";
    state.deviceError = "";
    draw();
    try {
      state.candidates = await discoverDevices(client, 2500);
      state.devices = "done";
    } catch (e) {
      state.candidates = [];
      state.devices = "failed";
      state.deviceError = e && e.message ? e.message : String(e);
    }
    draw();
  }

  async function loadMedia() {
    state.media = "looking";
    try {
      const listing = await client.call("media.list", {});
      state.items = (listing && listing.items) || [];
      state.media = "done";
    } catch (e) {
      state.items = [];
      state.media = "failed";
      state.mediaError = e && e.message ? e.message : String(e);
    }
    draw();
  }
}

// ------------------------------------------------------------ the tile grid

/**
 * The older picker, still what an output gets: every kind as a tile, grouped,
 * with a search box and a list view for a narrow window.
 */
function openTiles(client, what, kinds, opts) {
  let listView = false;
  try {
    listView = localStorage.getItem(LIST_KEY) === "1";
  } catch {
    /* no storage, no preference */
  }

  const search = el("input", { type: "text", placeholder: "Search", "aria-label": "Search kinds" });
  const grid = el("div.picker-grid");
  const body = el("div", {}, [search, grid]);

  const toggle = el("button.btn", {
    text: listView ? "Tiles" : "List",
    onclick: () => {
      listView = !listView;
      toggle.textContent = listView ? "Tiles" : "List";
      try {
        localStorage.setItem(LIST_KEY, listView ? "1" : "0");
      } catch {
        /* nothing to remember it with */
      }
      draw();
    },
  });

  const m = modal({
    title: what === "output" ? "Add an output" : "Add a source",
    body,
    footer: [toggle, el("button.btn", { text: "Cancel", onclick: () => m.close() })],
    wide: true,
  });

  function draw() {
    const q = search.value.trim().toLowerCase();
    const matching = kinds.filter(
      (k) =>
        !q ||
        k.title.toLowerCase().includes(q) ||
        k.description.toLowerCase().includes(q) ||
        k.id.toLowerCase().includes(q) ||
        String(k.plugin).toLowerCase().includes(q)
    );
    clear(grid);
    grid.className = "picker-grid" + (listView ? " list" : "");
    if (!matching.length) {
      grid.appendChild(el("p.dim", { text: `Nothing matches "${search.value}".` }));
      return;
    }
    for (const [group, items] of grouped(matching)) {
      const section = el("div");
      section.appendChild(el("div.group-title", { text: group }));
      const inner = el("div.picker-grid" + (listView ? ".list" : ""));
      for (const kind of items) inner.appendChild(tile(client, what, kind, opts, () => m.close()));
      section.appendChild(inner);
      grid.appendChild(section);
    }
  }

  on(search, "input", draw);
  draw();
  search.focus();
  return m;
}

/** One kind, as a tile that opens its form. */
function tile(client, what, kind, opts, close) {
  return el(
    "button.kindtile",
    {
      onclick: () => {
        close();
        openForm(client, what, kind, opts.preset, opts);
      },
    },
    [
      svg(ICONS[kind.icon] || ICONS.stream, 26),
      el("span.grow", {}, [
        el("div", { text: kind.title }),
        el("div.dim.sm", { text: kind.description }),
        el("div.who", { text: kind.plugin }),
      ]),
    ]
  );
}

/**
 * The chosen kind's form, rendered from its schema and nothing else.
 *
 * The schema reader is fetched here rather than with the page: eleven
 * kilobytes that matter only once somebody is adding something, and the picker
 * itself is already a modal the operator waited a moment for. A plugin's kind
 * carries a function rather than a schema, because reading it is another call
 * and nobody wanted it until now.
 */
export async function openForm(client, what, kind, preset, opts = {}) {
  const { SchemaForm } = await import("../client/schema-form.js");
  let schema = typeof kind.schema === "function" ? await kind.schema() : kind.schema;
  // Only for a kind a plugin provides. A file, a page and a stream have no
  // devices behind them, and asking would hold their forms for nothing.
  if (what === "source" && kind.plugin && schema?.properties) {
    // Short, and its failure is nothing: the box stays a box.
    const found = await discoverDevices(client, 1500).catch(() => []);
    schema = withDeviceChoices(schema, kind.id, found, preset || {});
  }
  const form = new SchemaForm(schema, preset || {});
  const add = el("button.btn.primary", { text: what === "output" ? "Start sending" : "Add" });

  const m = modal({
    title: kind.title,
    body: el("div", {}, [el("p.dim.sm", { text: kind.description, style: { marginTop: "0" } }), form.el]),
    footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() }), add],
  });

  on(form.el, "keydown", (e) => {
    if (e.key === "Enter" && e.target.tagName === "INPUT") {
      e.preventDefault();
      add.click();
    }
  });

  add.onclick = async () => {
    if (!form.validate()) {
      toast({ kind: "warning", text: "Fill in the fields outlined in red." });
      return;
    }
    add.disabled = true;
    let answer;
    try {
      const params = kind.build(form.read());
      answer = await client.call(what === "output" ? "output.add" : "source.add", params);
    } catch (e) {
      errorToast(e, kind.title);
      add.disabled = false;
      return;
    }
    m.close();
    toast({ text: what === "output" ? "Sending started." : "Added. It appears as soon as it connects." });
    // Whatever the caller wanted doing with the thing that now exists. It is
    // run outside the try on purpose: the add succeeded, the form is gone, and
    // a failure in the follow up is the caller's to explain.
    if (opts.onAdded) await opts.onAdded(answer);
  };
  form.focusFirst();
  return m;
}

/**
 * Something was dropped on the window. A file, a URL or an RTMP address skips
 * the picker: the scheme picks the kind, exactly as the URL detection in the
 * old page did, and the form opens with the address already filled in.
 */
export function pickFromDrop(client, text) {
  const uri = String(text || "").trim();
  if (!uri) return;
  const id = kindOfUri(uri);
  const kind = SOURCE_KINDS.find((k) => k.id === id) || SOURCE_KINDS[0];
  openForm(client, "source", kind, { uri });
}
