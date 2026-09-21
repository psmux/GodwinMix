import { el, clear } from '../../shell/dom.js';
import { errorToast } from '../../shell/toast.js';
import { openPicker } from '../../shell/picker.js';
import { nameOf } from './local.js';
import { sourcePreview } from './chooser-preview.js';

/** Reuse a mixer source in a scene without creating another capture pipeline. */
export async function openSceneSources(client, scenes, scene) {
  const preview = sourcePreview(client);
  let query = '';
  const list = el('div.source-chooser-list', { role: 'list', 'aria-label': 'Available sources' });
  const added = new Set(scene.sources || []);
  const pending = new Set();
  let signature = '', closed = false;
  const add = async source => {
    if (pending.has(source.id) || added.has(source.id)) return;
    pending.add(source.id);
    render(true);
    try {
      await scenes.itemAdd(scene.id, { source: source.id });
      added.add(source.id);
      scenes.undo.record(`Added ${nameOf(source)} to ${scene.name}`);
      await scenes.reread([scene.id]);
    } finally {
      pending.delete(source.id);
      if (!closed) render(true);
    }
  };
  function row(source) {
    const present = added.has(source.id);
    return el('div.source-choice', { role: 'listitem' }, [
      el('div.grow', {}, [el('strong', { text: nameOf(source) }), el('div.sm.dim', { text: source.type || source.state || source.id })]),
      el('button.btn', { text: 'Preview', 'aria-label': `Preview ${nameOf(source)}`, onclick: () => { preview.select(source); preview.node.scrollIntoView({ block: 'nearest', behavior: 'smooth' }); } }),
      el('button.btn.primary', { text: pending.has(source.id) ? 'Adding…' : present ? 'In scene' : 'Add',
        'aria-label': `${present ? 'Already in scene:' : 'Add'} ${nameOf(source)}`, disabled: present || pending.has(source.id),
        onclick: () => add(source).catch(error => errorToast(error, `Add to ${scene.name}`)) }),
    ]);
  }
  function render(force = false) {
    if (closed) return;
    const sources = client.state.sources || [];
    const next = sources.map(s => `${s.id}/${nameOf(s)}/${s.state}`).join('|') + query;
    if (!force && next === signature) return;
    signature = next;
    const shown = sources.filter(s => `${nameOf(s)} ${s.id} ${s.type || ''}`.toLowerCase().includes(query));
    clear(list);
    list.append(...shown.map(row));
    if (!shown.length) list.append(el('p.dim', { text: query ? 'No matching sources.' : 'No existing sources. Choose a camera, device or file from the categories.' }));
  }
  const body = el('div.source-chooser', {}, [
    el('div.col.source-chooser-library', {}, [list]), preview.node,
  ]);
  const off = client.onRender(() => render());
  const close = () => { closed = true; off(); preview.destroy(); };
  // Open on what the mixer already has when some of it is not in this scene.
  // A second scene nearly always wants a camera the first one uses, and the
  // list of cameras to set up from nothing is the wrong page to start on.
  const unused = (client.state.sources || []).some(source => !added.has(source.id));
  try {
    return await openPicker(client, 'source', {
      title: `Add sources to ${scene.name}`, category: unused ? 'existing' : 'cameras',
      existing: { node: body, draw(value) { query = value.trim().toLowerCase(); render(true); }, deactivate() { preview.select(null); } },
      contains: source => added.has(source.id) || (scenes.summary?.(scene.id)?.sources || []).includes(source.id),
      onExisting: add, onAdded: add, onClose: close,
    });
  } catch (error) { close(); throw error; }
}
