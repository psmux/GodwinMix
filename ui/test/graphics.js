import { addRequest, changedOnly, colourInput, ordered, placementFor, setRequest, isGraphic } from '../panels/sources/graphic-fields.js';
import { openGraphicEditor } from '../panels/sources/graphic-editor.js';

const settle = (ms) => new Promise(r => setTimeout(r, ms));

const STRAP = {
  name: 'news-lower-third', title: 'News lower third', description: 'A name and a title', origin: 'pack',
  uri: 'template:news-lower-third', width: 1920, height: 1080,
  fields: [
    { name: 'accent', label: 'Accent colour', type: 'color', default: '#d4202c' },
    { name: 'name', label: 'Name', type: 'text', default: 'Ada Lovelace', fit: 936 },
    { name: 'title', label: 'Title', type: 'text', default: 'Analyst' },
  ],
};

/** A client that answers from a table and remembers what it was asked. */
function stub(answers) {
  return {
    calls: [],
    state: { sources: [] },
    call(method, params) {
      this.calls.push([method, params]);
      const a = answers[method];
      if (a === undefined) return Promise.reject(new Error(`no stub for ${method}`));
      return Promise.resolve(typeof a === 'function' ? a(params) : a);
    },
    sent(method) { return this.calls.filter(c => c[0] === method).map(c => c[1]); },
  };
}

export async function graphicTests(test, eq, ok) {
  test('a graphic is a template source and nothing else', () => {
    ok(isGraphic({ type: 'template/source' })); ok(!isGraphic({ type: 'image/source' })); ok(!isGraphic(null));
  });
  test('words come before colours in a form', () => eq(ordered(STRAP.fields).map(f => f.name).join(), 'name,title,accent'));
  test('a colour input gets six digits whatever the template wrote', () => {
    eq(colourInput('#abc'), '#aabbcc'); eq(colourInput('#D4202CFF'), '#d4202c'); eq(colourInput('red'), '#000000');
  });
  test('only what was changed is sent, so the brand colours still apply', () => {
    eq(JSON.stringify(changedOnly({ name: 'A', accent: '#d4202c' }, { name: 'B', accent: '#d4202c' })), '{"name":"B"}');
    eq(JSON.stringify(addRequest(STRAP, {})), '{"uri":"template:news-lower-third","name":"News lower third","type":"template/source"}');
    eq(addRequest(STRAP, { name: 'B' }).params.fields.name, 'B');
  });
  test('a graphic covers the canvas and a field change touches params.fields alone', () => {
    const t = placementFor({ width: 1280, height: 720 });
    eq(t.frame.w, 1280); eq(t.frame.h, 720); eq(t.position.x, 0);
    eq(JSON.stringify(setRequest('strap', { title: null })), '{"id":"strap","params":{"fields":{"title":null}}}');
  });

  // Add a source, Graphics, Choose, type a name, Add.
  const { openPicker } = await import('../shell/picker-loader.js');
  const client = stub({
    'plugin.list': { plugins: [] }, 'core.api': { kinds: { source: [] } }, 'device.discover': { candidates: [] },
    'media.list': { items: [] }, 'template.list': { templates: [STRAP] },
    'core.info': { canvas: { width: 1280, height: 720 } },
    'source.add': (p) => ({ id: 'strap', uri: p.uri, type: p.type }),
  });
  const added = [];
  const picker = await openPicker(client, 'source', { category: 'graphics', onAdded: (a) => added.push(a) });
  await settle(50);
  const choose = [...picker.el.querySelectorAll('.picker-panel button')].find(b => b.textContent === 'Choose');
  test('Graphics lists the templates the mixer has', () => {
    ok(picker.el.querySelector('.picker-panel').textContent.includes('News lower third'));
    ok(choose, 'a Choose button');
  });
  choose && choose.click();
  await settle(20);
  const form = [...document.querySelectorAll('.dialog')].pop();
  const boxes = [...form.querySelectorAll('input')];
  test('the form asks for each field, filled with its default', () => {
    eq(boxes.map(b => b.getAttribute('aria-label')).join(), 'Name,Title,Accent colour');
    eq(boxes[0].value, 'Ada Lovelace'); eq(boxes[2].value, '#d4202c');
  });
  boxes[0].value = 'Grace Hopper & co';
  boxes[0].dispatchEvent(new Event('input'));
  [...form.querySelectorAll('footer button')].find(b => b.textContent === 'Add').click();
  await settle(50);
  test('Add sends the changed field and places the graphic over the canvas', () => {
    const req = client.sent('source.add')[0];
    ok(req, JSON.stringify(client.calls));
    eq(req.uri, 'template:news-lower-third'); eq(JSON.stringify(req.params), '{"fields":{"name":"Grace Hopper & co"}}');
    eq(added.length, 1); eq(added[0].placement.frame.w, 1280);
  });

  // The settings drawer: change a field, then put another back.
  const slot = document.body.appendChild(document.createElement('div'));
  slot.className = 'slot-sidebar';
  const live = stub({
    'template.fields': { id: 'strap', template: 'news-lower-third', path: 'params.fields.<name>',
      fields: STRAP.fields.map(f => ({ ...f, value: f.name === 'name' ? 'Grace' : f.default, set: f.name === 'name' })) },
    'source.set': {},
  });
  await openGraphicEditor({ client: live }, { id: 'strap', name: 'Strap', type: 'template/source' });
  const title = slot.querySelector('input[aria-label="Title"]');
  test('the editor opens on what each field shows now', () => eq(slot.querySelector('input[aria-label="Name"]').value, 'Grace'));
  title.value = 'Rear Admiral';
  title.dispatchEvent(new Event('input'));
  slot.querySelector('button[aria-label="Default Name"]').click();
  await settle(260);
  test('one source.set carries the typed field and the reset one, nothing else', () => {
    const sets = live.sent('source.set');
    eq(sets.length, 1); eq(JSON.stringify(sets[0]), '{"id":"strap","params":{"fields":{"title":"Rear Admiral","name":null}}}');
  });
  slot.remove();
  document.body.classList.remove('drawer-open');
}
