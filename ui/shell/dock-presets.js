import { el } from './dom.js';
import { clean, leaves, save } from './dock-model.js';

const KEY = 'gmx.workspace.layouts.v1';
function stored() {
  try {
    const value = JSON.parse(localStorage.getItem(KEY));
    return Array.isArray(value) ? value.filter(x => typeof x.name === 'string').slice(0, 20) : [];
  } catch { return []; }
}
export function decodeLayout(value) {
  if (!value || value.version !== 1 || !Object.hasOwn(value, 'tree')) throw new Error('Choose a workspace JSON file exported by GodwinMix.');
  const tree = clean(value.tree);
  if (value.tree && !tree) throw new Error('This workspace has no valid panels. Export it again from its original browser.');
  const present = new Set(leaves(tree).flatMap(n => n.tabs));
  const hidden = Array.isArray(value.hidden) ? [...new Set(value.hidden.filter(id => typeof id === 'string' && !present.has(id)))] : [];
  return { tree, hidden };
}
function snapshot(workspace) {
  return JSON.parse(JSON.stringify({ version: 1, ...workspace.state }));
}
function apply(workspace, value) {
  workspace.state = decodeLayout(value);
  workspace.sync();
  save(workspace.state);
}
export function savedLayouts(workspace, close) {
  const name = el('input', { type: 'text', maxlength: 64, placeholder: 'Sunday service', 'aria-label': 'Layout name' });
  const status = el('p.sm.dim', { role: 'status' });
  const select = el('select', { 'aria-label': 'Saved workspace' });
  const refresh = () => select.replaceChildren(...stored().map(x => el('option', { value: x.name, text: x.name })));
  const report = action => { try { action(); } catch (error) { status.textContent = error.message; } };
  refresh();
  const saveButton = el('button.btn', { text: 'Save current', onclick: () => report(() => {
    const title = name.value.trim();
    if (!title) throw new Error('Enter a name for this layout.');
    const layouts = stored().filter(x => x.name !== title);
    if (layouts.length >= 20) throw new Error('Twenty layouts are saved. Delete one before saving another.');
    layouts.push({ name: title, ...snapshot(workspace) });
    localStorage.setItem(KEY, JSON.stringify(layouts));
    refresh(); select.value = title; status.textContent = 'Layout saved on this device.';
  }) });
  const loadButton = el('button.btn', { text: 'Load', onclick: () => report(() => {
    const value = stored().find(x => x.name === select.value);
    if (!value) throw new Error('Save a layout first.');
    apply(workspace, value); close();
  }) });
  const deleteButton = el('button.btn', { text: 'Delete saved', onclick: () => report(() => {
    localStorage.setItem(KEY, JSON.stringify(stored().filter(x => x.name !== select.value)));
    refresh(); status.textContent = 'Saved layout removed. The current workspace is unchanged.';
  }) });
  const file = el('input', { type: 'file', accept: '.json,application/json', hidden: true });
  file.addEventListener('change', async () => {
    const selected = file.files?.[0];
    if (!selected) return;
    if (selected.size > 65536) { status.textContent = 'Workspace files must be smaller than 64 KB.'; return; }
    try { apply(workspace, JSON.parse(await selected.text())); close(); }
    catch (error) { status.textContent = 'Workspace was not loaded. ' + error.message; }
    file.value = '';
  });
  const exportButton = el('button.btn', { text: 'Export JSON', onclick: () => {
    const url = URL.createObjectURL(new Blob([JSON.stringify(snapshot(workspace), null, 2)], { type: 'application/json' }));
    const link = el('a', { href: url, download: 'godwinmix-workspace.json' });
    link.click(); setTimeout(() => URL.revokeObjectURL(url), 1000);
  } });
  return el('section.col', {}, [el('h3', { text: 'Saved workspaces' }),
    el('div.row', {}, [name, saveButton]), el('div.row', {}, [select, loadButton, deleteButton]),
    el('div.row', {}, [exportButton, el('button.btn', { text: 'Import JSON', onclick: () => file.click() })]),
    el('p.sm.dim', { text: 'Layouts contain panel positions and visibility. Mixer settings and credentials are not included.' }), status, file,
  ]);
}
