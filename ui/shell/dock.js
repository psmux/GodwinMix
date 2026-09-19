import { el } from './dom.js';
import * as model from './dock-model.js';
import * as registry from './registry.js';
import { gestures } from './dock-pointer.js';
import { positionWorkspace } from './dock-geometry.js';
// Layout dialogs load only when an operator opens them.
import { lazyAction } from './lazy-action.js';
const workspaceMenu = lazyAction(() => import('./dock-menu.js').then(m => m.workspaceMenu), 'Open workspace controls');
const panelMenu = lazyAction(() => import('./dock-menu.js').then(m => m.panelMenu), 'Open panel controls');

export class Workspace {
  constructor(host, client, layout) {
    this.client = client;
    this.layout = layout;
    this.state = model.load(layout);
    this.frames = new Map();
    this.splits = new Map();
    this.root = el('section.dock-workspace', { 'aria-label': 'Broadcast workspace' });
    this.toolbar = el('nav.dock-toolbar', { 'aria-label': 'Workspace controls' }, [
      el('span', { text: 'WORKSPACE' }),
      el('button.btn.sm', { text: 'Panels and layout', onclick: () => workspaceMenu(this) }),
      el('span.dock-hint', { text: 'Drag a panel title to split or group. Use its menu for keyboard controls.' }),
    ]);
    host.append(this.toolbar, this.root);
    this.observer = new ResizeObserver(() => this.schedule());
    this.observer.observe(this.root);
    this.live = el('span.sr-only', { 'aria-live': 'polite' });
    this.root.append(this.live);
  }
  sync() {
    for (const spec of registry.list()) {
      if (!spec.slots.some(s => s === 'header' || s === 'modal')) continue;
      this.state.tree = model.remove(this.state.tree, spec.id);
      this.state.hidden = this.state.hidden.filter(id => id !== spec.id);
    }
    const known = new Set([...model.leaves(this.state.tree).flatMap(n => n.tabs), ...this.state.hidden]);
    for (const spec of registry.list()) {
      if (spec.slots.includes('header') || spec.slots.includes('modal') || known.has(spec.id)) continue;
      const utility = model.leaves(this.state.tree).find(n => n.tabs.includes('core/outputs'));
      if (utility) utility.tabs.push(spec.id);
      else this.state.tree = this.state.tree ? { axis: 'x', ratio: .75, a: this.state.tree, b: model.leaf([spec.id]) } : model.leaf([spec.id]);
    }
    this.render();
  }
  schedule() {
    if (!this.raf) this.raf = requestAnimationFrame(() => { this.raf = 0; this.position(); });
  }
  render() {
    const groups = model.leaves(this.state.tree);
    const active = new Set(groups.map(n => n.active));
    const present = new Set(groups.flatMap(n => n.tabs));
    for (const [id, frame] of this.frames) {
      if (active.has(id)) continue;
      if (present.has(id) && typeof frame.made.node.setWorkspaceActive === 'function') {
        try {
          if (!frame.element.hidden) frame.made.node.setWorkspaceActive(false);
          frame.element.hidden = true;
          continue;
        } catch (error) { console.error('Panel suspension failed', id, error); }
      }
      frame.made.destroy();
      frame.element.remove();
      this.frames.delete(id);
    }
    for (const group of groups) {
      const id = group.active;
      const stale = this.frames.get(id);
      if (stale?.made.unavailable && registry.get(id)) { stale.made.destroy(); stale.element.remove(); this.frames.delete(id); }
      if (!this.frames.has(id)) this.create(id);
      const frame = this.frames.get(id);
      if (!frame) continue;
      if (frame.element.hidden) {
        frame.element.hidden = false;
        try { frame.made.node.setWorkspaceActive(true); } catch (error) { console.error('Panel activation failed', id, error); }
      }
      frame.tabs.replaceChildren(...group.tabs.map(tab => el('button.dock-tab', {
        text: registry.get(tab)?.title || tab, role: 'tab', 'aria-selected': String(tab === id), tabindex: tab === id ? 0 : -1,
        onclick: () => this.activate(group, tab),
        onkeydown: e => {
          const delta = e.key === 'ArrowLeft' ? -1 : e.key === 'ArrowRight' ? 1 : 0;
          if (!delta) return;
          e.preventDefault();
          this.activate(group, group.tabs[(group.tabs.indexOf(tab) + delta + group.tabs.length) % group.tabs.length]);
        },
      })));
    }
    this.position();
    model.save(this.state);
  }
  activate(group, id) {
    group.active = id;
    this.render();
    this.frames.get(id)?.tabs.querySelector('[aria-selected="true"]')?.focus();
  }
  create(id) {
    const made = registry.instantiate(id, this.client, {}) || {
      node: el('p.dim.pad', { text: 'This panel is unavailable. Reload plugin panels or close it here.' }),
      unavailable: true, destroy() { this.node.remove(); },
    };
    const title = registry.get(id)?.title || id;
    const handle = el('button.dock-handle', { text: title, title: 'Drag to dock ' + title, 'aria-label': 'Move ' + title });
    const tabs = el('div.dock-tabs', { role: 'tablist', 'aria-label': title + ' group' });
    const bar = el('div.dock-title', {}, [handle,
      el('button', { text: '⋯', title: 'Panel actions', 'aria-label': title + ' panel actions', onclick: () => panelMenu(this, id) }),
      el('button', { text: '×', title: 'Close panel', 'aria-label': 'Close ' + title, onclick: () => this.hide(id) }),
    ]);
    const element = el('section.dock-frame', { 'data-dock-panel': id, 'aria-label': title }, [bar, tabs, el('div.dock-content', {}, [made.node])]);
    this.frames.set(id, { element, tabs, made });
    this.root.append(element);
    gestures(this, handle, id);
  }
  position() {
    positionWorkspace(this);
  }

  move(id, target, edge) {
    this.state.tree = model.dock(this.state.tree, id, target, edge);
    this.live.textContent = `${registry.get(id)?.title || id} moved ${edge === 'center' ? 'into group' : 'to ' + edge}`;
    this.render();
  }
  hide(id) {
    this.state.tree = model.remove(this.state.tree, id);
    if (!this.state.hidden.includes(id)) this.state.hidden.push(id);
    this.render();
    this.toolbar.querySelector('button').focus();
  }
  show(id) {
    this.state.hidden = this.state.hidden.filter(x => x !== id);
    const group = model.leaves(this.state.tree)[0];
    if (group) { group.tabs.push(id); group.active = id; }
    else this.state.tree = model.leaf([id]);
    this.render();
  }
  reset() {
    this.state = { tree: model.initial(this.layout), hidden: [] };
    this.sync();
  }
  destroy() {
    this.observer.disconnect();
    cancelAnimationFrame(this.raf);
    for (const frame of this.frames.values()) frame.made.destroy();
    this.frames.clear();
  }
}
export function place(element, r) {
  Object.assign(element.style, { left: r.x + 'px', top: r.y + 'px', width: Math.max(0, r.w) + 'px', height: Math.max(0, r.h) + 'px' });
}
