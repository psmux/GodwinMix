// What each item type is, and what its plugin said about editing it.
//
// A plugin contributes a `designer` block to a provide (11 section 5): an icon,
// a UI schema, a default frame, the gizmos it has, its snap geometry and,
// optionally, an editor element. None of it is code. This file is how the
// composer finds that block for the item under the cursor.
//
// Two calls, both cached for the life of the composer:
//
//   plugin.list      every instance, with the plugin and the provide it is
//   plugin.describe  that plugin's manifest and the settings schema per provide
//
// A core that answers neither leaves every item on the default handles, which
// is what an item with no plugin behind it gets anyway.

export class Catalogue {
  constructor(client) {
    this.client = client;
    /** instance id -> {plugin, provide} */
    this.instances = new Map();
    /** "plugin/provide" -> {designer, schema, title} */
    this.types = new Map();
    this.ready = null;
  }

  load() {
    if (!this.ready) this.ready = this.read().catch(() => {});
    return this.ready;
  }

  async read() {
    let listing;
    try {
      listing = await this.client.call("plugin.list", {});
    } catch {
      return;
    }
    const names = new Set();
    for (const plugin of (listing && listing.plugins) || []) {
      for (const instance of plugin.instances || []) {
        if (!instance.instance) continue;
        this.instances.set(instance.instance, {
          plugin: instance.plugin || plugin.name,
          provide: instance.provide || null,
        });
        names.add(instance.plugin || plugin.name);
      }
    }
    for (const name of names) await this.describe(name);
  }

  async describe(name) {
    let described;
    try {
      described = await this.client.call("plugin.describe", { id: name });
    } catch {
      return;
    }
    const manifest = (described && described.manifest) || {};
    const schemas = (described && described.schemas) || {};
    for (const provide of manifest.provides || []) {
      const key = `${name}/${provide.id}`;
      this.types.set(key, {
        plugin: name,
        provide: provide.id,
        title: provide.title || provide.id,
        designer: provide.designer || null,
        schema: schemas[key] || null,
      });
    }
  }

  /**
   * The type behind one item record, or null.
   *
   * An item draws a source, a nested scene or a graphic. Only a source and a
   * graphic have a plugin behind them; a reference is the mixer's own and gets
   * the default handles.
   */
  typeOf(record) {
    const content = (record && record.content) || {};
    if (content.source) {
      const where = this.instances.get(content.source);
      if (!where) return null;
      const key = where.provide ? `${where.plugin}/${where.provide}` : null;
      return (key && this.types.get(key)) || this.byPlugin(where.plugin);
    }
    if (content.graphic) return this.types.get(content.graphic) || null;
    return null;
  }

  /** A plugin with one provide is unambiguous, which covers most of them. */
  byPlugin(name) {
    const mine = [...this.types.values()].filter((t) => t.plugin === name);
    return mine.length === 1 ? mine[0] : null;
  }

  designerFor(record) {
    const type = this.typeOf(record);
    return type ? type.designer : null;
  }
}
