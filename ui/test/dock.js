import * as model from '../shell/dock-model.js';
import { Workspace } from '../shell/dock.js';
import { registerElement } from '../shell/registry.js';
import { decodeLayout } from '../shell/dock-presets.js';

export function dockTests(test, eq, ok) {
  test('workspace imports validate versions and normalize visible and hidden panels', () => {
    const loaded = decodeLayout({ version: 1, tree: model.leaf(['a', 'a']), hidden: ['a', 'b', 'b', null] });
    eq(loaded.tree.tabs, ['a']); eq(loaded.hidden, ['b']);
    let rejected = false;
    try { decodeLayout({ version: 9, tree: null }); } catch { rejected = true; }
    ok(rejected);
  });
  test('dock splits nest and retain exactly one copy of each panel', () => {
    let tree = model.leaf(['a', 'b', 'c']);
    tree = model.dock(tree, 'b', 'a', 'left');
    tree = model.dock(tree, 'c', 'a', 'bottom');
    eq(tree.axis, 'x');
    eq(tree.b.axis, 'y');
    eq(model.leaves(tree).flatMap(n => n.tabs).sort(), ['a', 'b', 'c']);
    tree = model.dock(tree, 'b', 'c', 'center');
    eq(model.leaves(tree).find(n => n.tabs.includes('b')).active, 'b');
  });
  test('closing a last panel collapses empty split branches', () => {
    const tree = model.dock(model.leaf(['a', 'b']), 'b', 'a', 'right');
    eq(model.remove(tree, 'a'), model.leaf(['b']));
    eq(model.remove(model.leaf(['b']), 'b'), null);
  });
  test('saved dock trees reject duplicate ids and clamp split ratios', () => {
    const tree = model.clean({ axis: 'x', ratio: 9, a: model.leaf(['a']), b: model.leaf(['a', 'b']) });
    eq(tree.ratio, .85);
    eq(tree.b.tabs, ['b']);
    eq(model.rectangles(tree, { x: 0, y: 0, w: 1000, h: 600 }).panels.length, 2);
  });
  test('dock migration includes plugin slots and excludes fixed shell panels', () => {
    const tree = model.initial({ header: ['header'], monitor: ['core/program'], main: ['core/scenes'], sidebar: ['plugin/tool'], modal: ['modal'] });
    eq(model.leaves(tree).flatMap(n => n.tabs).sort(), ['core/program', 'core/scenes', 'plugin/tool']);
  });
  test('moving and resizing preserve connected panel identity and hiding releases it', () => {
    let connected = 0, disconnected = 0;
    class Probe extends HTMLElement {
      static get panel() { return { id: 'test/dock-probe', title: 'Probe', slots: ['main'] }; }
      connectedCallback() { connected++; }
      disconnectedCallback() { disconnected++; }
    }
    class Other extends HTMLElement {
      static get panel() { return { id: 'test/dock-other', title: 'Other', slots: ['main'] }; }
    }
    registerElement(Probe); registerElement(Other);
    const saved = localStorage.getItem(model.KEY);
    localStorage.removeItem(model.KEY);
    const host = document.createElement('div'); document.body.append(host);
    const workspace = new Workspace(host, {}, { main: ['test/dock-probe', 'test/dock-other'] });
    workspace.state.tree = model.dock(model.leaf(['test/dock-probe', 'test/dock-other']), 'test/dock-probe', 'test/dock-other', 'left');
    try {
      workspace.render();
      const original = workspace.frames.get('test/dock-probe').made.node;
      workspace.move('test/dock-probe', 'test/dock-other', 'bottom');
      workspace.position();
      ok(workspace.frames.get('test/dock-probe').made.node === original);
      eq(connected, 1); eq(disconnected, 0);
      workspace.hide('test/dock-probe');
      eq(disconnected, 1);
      workspace.show('test/dock-probe');
      eq(connected, 2);
      ok(workspace.frames.get('test/dock-probe').made.node !== original);
    } finally {
      workspace.destroy(); host.remove();
      if (saved === null) localStorage.removeItem(model.KEY); else localStorage.setItem(model.KEY, saved);
    }
  });
  test('optional suspension preserves local panel state across tab switches', () => {
    const calls = [];
    class Suspended extends HTMLElement {
      static get panel() { return { id: 'test/dock-suspend', title: 'Suspend', slots: ['main'] }; }
      setWorkspaceActive(active) { calls.push(active); }
    }
    registerElement(Suspended);
    const saved = localStorage.getItem(model.KEY);
    const host = document.createElement('div'); document.body.append(host);
    const workspace = new Workspace(host, {}, {});
    workspace.state.tree = model.leaf(['test/dock-suspend', 'test/dock-other']);
    try {
      workspace.render();
      const original = workspace.frames.get('test/dock-suspend').made.node;
      original.formValue = 'unfinished';
      workspace.activate(workspace.state.tree, 'test/dock-other');
      eq(calls, [false]);
      ok(original.isConnected);
      workspace.activate(workspace.state.tree, 'test/dock-suspend');
      eq(calls, [false, true]);
      eq(workspace.frames.get('test/dock-suspend').made.node.formValue, 'unfinished');
      workspace.hide('test/dock-suspend');
      ok(!original.isConnected);
    } finally {
      workspace.destroy(); host.remove();
      if (saved === null) localStorage.removeItem(model.KEY); else localStorage.setItem(model.KEY, saved);
    }
  });
  test('imported fixed panels stay fixed and unavailable plugins can recover', () => {
    class Fixed extends HTMLElement {
      static get panel() { return { id: 'test/fixed', title: 'Fixed', slots: ['header'] }; }
    }
    class Late extends HTMLElement {
      static get panel() { return { id: 'test/late', title: 'Late', slots: ['main'] }; }
    }
    registerElement(Fixed);
    const saved = localStorage.getItem(model.KEY);
    const host = document.createElement('div'); document.body.append(host);
    const workspace = new Workspace(host, {}, {});
    workspace.state.tree = { axis: 'x', ratio: .5, a: model.leaf(['test/fixed']), b: model.leaf(['test/late']) };
    try {
      workspace.sync();
      ok(!workspace.frames.has('test/fixed'));
      ok(workspace.frames.get('test/late').made.unavailable);
      registerElement(Late);
      workspace.sync();
      ok(workspace.frames.get('test/late').made.node instanceof Late);
      eq(host.querySelectorAll('[data-panel="test/late"]').length, 1);
    } finally {
      workspace.destroy(); host.remove();
      if (saved === null) localStorage.removeItem(model.KEY); else localStorage.setItem(model.KEY, saved);
    }
  });
}
