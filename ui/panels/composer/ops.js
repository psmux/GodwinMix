// The composer's semantic commands.
//
// Every one is one call that cannot produce an off by twelve pixels result (11
// section 4), so the toolbar is a list of names rather than a set of number
// boxes, and an agent reaching for the same operation gets exactly the same
// arithmetic. Nothing here computes a position: that is the point of them.
//
// `duration_ms: 0` throughout. A composer edit is a cut on a draft nobody is
// watching; durations belong to `scene.apply_layout` and to a take.

/**
 * @param {object} scenes a SceneClient
 * @param {() => {scene: string, draft: ?string, items: string[]}} context
 */
export function operations(scenes, context) {
  const call = (method, params) => {
    const c = context();
    const body = Object.assign({ scene: c.scene }, params);
    if (c.draft) body.draft = c.draft;
    return scenes.call(method, body);
  };
  const items = () => context().items;
  const one = () => context().items[0];

  return [
    { id: "align-left", group: "Align", title: "Left", min: 2, run: () => call("scene.item.align", { items: items(), edge: "left" }) },
    { id: "align-center-x", group: "Align", title: "Centre across", min: 2, run: () => call("scene.item.align", { items: items(), edge: "center-x" }) },
    { id: "align-right", group: "Align", title: "Right", min: 2, run: () => call("scene.item.align", { items: items(), edge: "right" }) },
    { id: "align-top", group: "Align", title: "Top", min: 2, run: () => call("scene.item.align", { items: items(), edge: "top" }) },
    { id: "align-center-y", group: "Align", title: "Centre down", min: 2, run: () => call("scene.item.align", { items: items(), edge: "center-y" }) },
    { id: "align-bottom", group: "Align", title: "Bottom", min: 2, run: () => call("scene.item.align", { items: items(), edge: "bottom" }) },

    { id: "distribute-h", group: "Space", title: "Across", min: 3, run: () => call("scene.item.distribute", { items: items(), axis: "horizontal" }) },
    { id: "distribute-v", group: "Space", title: "Down", min: 3, run: () => call("scene.item.distribute", { items: items(), axis: "vertical" }) },

    { id: "fit", group: "Size", title: "Fit the canvas", min: 1, run: () => call("scene.item.fit_to_canvas", { items: items() }) },
    { id: "cover", group: "Size", title: "Cover the canvas", min: 1, run: () => call("scene.item.cover_canvas", { items: items() }) },
    { id: "grid", group: "Size", title: "Arrange in a grid", min: 2, run: () => call("scene.item.arrange_grid", { items: items(), cols: Math.ceil(Math.sqrt(items().length)) }) },
    { id: "match", group: "Size", title: "Match the first one's size", min: 2, run: () => call("scene.item.match_size", { items: items().slice(1), to: one() }) },

    { id: "group", group: "Structure", title: "Group", min: 2, run: () => call("scene.item.group", { items: items() }) },
    { id: "ungroup", group: "Structure", title: "Ungroup", min: 1, run: () => call("scene.item.ungroup", { item: one() }) },
    { id: "front", group: "Structure", title: "Bring to front", min: 1, run: () => reorder(scenes, context, "front") },
    { id: "back", group: "Structure", title: "Send to back", min: 1, run: () => reorder(scenes, context, "back") },
    { id: "remove", group: "Structure", title: "Remove from the scene", min: 1, destructive: true, run: () => call("scene.item.remove", { item: one() }) },
  ];
}

/**
 * Up or down the stack.
 *
 * `scene.item.reorder` takes the neighbours rather than an index, because an
 * index moves when somebody else adds an item and a neighbour does not.
 */
function reorder(scenes, context, where) {
  const c = context();
  const view = scenes.view(c.scene);
  const siblings = ((view && view.records) || []).filter((r) => r.kind === "item" && r.parent === c.scene);
  if (siblings.length < 2) return Promise.resolve();
  const item = c.items[0];
  const body = { scene: c.scene, item };
  if (c.draft) body.draft = c.draft;
  if (where === "front") body.after = siblings[siblings.length - 1].id;
  else body.before = siblings[0].id;
  if (body.after === item || body.before === item) return Promise.resolve();
  return scenes.call("scene.item.reorder", body);
}

/** The properties the inspector's own row of controls assigns. */
export function itemProps(scenes, context) {
  const set = (props) => {
    const c = context();
    return scenes.itemSet(c.scene, c.items[0], props, { duration_ms: 0, draft: c.draft });
  };
  return {
    opacity: (v) => set({ opacity: Number(v) }),
    visible: (v) => set({ visible: !!v }),
    locked: (v) => set({ locked: !!v }),
    blend: (v) => set({ blend: String(v) }),
    audio: (v) => set({ audio: String(v) }),
    fit: (v) => set({ transform: { fit: String(v) } }),
    name: (v) => set({ name: String(v) }),
  };
}

/** Blend modes and audio modes, as the document spells them. */
export const BLENDS = ["normal", "add", "screen", "multiply", "lighten", "darken", "subtract"];
export const AUDIO = ["follow", "always", "never"];
export const FITS = ["none", "contain", "cover", "stretch", "fit-width", "fit-height", "max"];

/**
 * Per item filters. A camera keyed in one scene is not keyed in all of them,
 * which is the whole reason filters hang on the item rather than the source.
 */
export function filters(scenes, context) {
  const body = (extra) => {
    const c = context();
    const out = Object.assign({ scene: c.scene, item: c.items[0] }, extra);
    if (c.draft) out.draft = c.draft;
    return out;
  };
  return {
    add: (type, params) => scenes.call("scene.item.filter.add", body({ type, params: params || {} })),
    set: (filter, params, enabled) => scenes.call("scene.item.filter.set", body({ filter, params, enabled })),
    remove: (filter) => scenes.call("scene.item.filter.remove", body({ filter })),
  };
}

/**
 * What filter types this core has, from the plugin list.
 *
 * Asked once when the filter menu is first opened. A core that answers nothing
 * gets the one filter every build has, so the menu is never empty and never
 * lies about what is there.
 */
export async function filterTypes(client) {
  try {
    const list = await client.call("plugin.list", {});
    const found = [];
    for (const plugin of (list && list.plugins) || []) {
      for (const provide of plugin.provides || []) {
        if (provide.kind === "filter") found.push({ id: `${plugin.name}/${provide.id}`, title: provide.title || provide.id, plugin: plugin.name });
      }
    }
    if (found.length) return found;
  } catch {
    /* an older core with no plugin.list falls through to the built in list */
  }
  return [{ id: "chroma/filter", title: "Chroma key", plugin: "built in" }];
}
