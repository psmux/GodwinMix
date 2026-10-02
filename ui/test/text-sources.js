import { TEXT_PRESETS, addRequest, placementFor, isText } from '../panels/sources/text-presets.js';
import { current, paramsFor, withAlpha, splitAlpha, fieldsFor } from '../panels/sources/text-fields.js';
import { openSceneSources } from '../panels/sources/chooser.js';

export async function textSourceTests(test, eq, ok) {
  const strap = TEXT_PRESETS.find(p => p.key === 'lower-third');
  const ticker = TEXT_PRESETS.find(p => p.key === 'ticker');
  test('a lower third is a text source with its words in params', () => {
    const req = addRequest(strap);
    eq(req.type, 'text/source'); eq(req.uri, 'text:'); ok(req.params.text.includes('\n'));
  });
  test('the lower third sits in the bottom left of the canvas and keeps its shape', () => {
    const t = placementFor(strap, { width: 1280, height: 720 });
    ok(t.position.y > 720 / 2 && t.position.x < 1280 / 4, JSON.stringify(t)); eq(t.fit, 'contain');
  });
  test('the ticker runs the whole width of the bottom of the frame', () => {
    const t = placementFor(ticker, { width: 1920, height: 1080 });
    eq(t.frame.w, 1920); eq(t.position.y + t.frame.h, 1080); eq(t.fit, 'stretch');
  });
  test('the editor opens on the source params over the defaults', () => {
    const v = current('ticker/source', { items: ['One', 'Two'], speed: 300 });
    eq(v.words, 'One\nTwo'); eq(v.speed, 300); eq(v.font, 'Sans');
    ok(fieldsFor('ticker/source').some(f => f.key === 'speed')); ok(!fieldsFor('text/source').some(f => f.key === 'speed'));
  });
  test('words become text for a text and items for a ticker', () => {
    eq(JSON.stringify(paramsFor('text/source', 'words', 'A\nB')), '{"text":"A\\nB"}');
    eq(JSON.stringify(paramsFor('ticker/source', 'words', 'A\nB')), '{"items":["A","B"]}');
  });
  test('a box colour carries its opacity in two more digits', () => {
    eq(withAlpha('#102030', 0.5), '#10203080'); eq(splitAlpha('#10203080').hex, '#102030');
    ok(Math.abs(splitAlpha('#10203080').opacity - 0.5) < 0.01);
  });
  test('only text and ticker sources open the text editor', () => {
    ok(isText({ type: 'text/source' })); ok(isText({ type: 'ticker/source' })); ok(!isText({ type: 'file/source' }));
  });

  // Add sources, then Add text: two presses, and the item lands where the
  // preset says rather than in the next grid cell.
  const calls = [];
  const client = {
    state: { sources: [] },
    call: async (method, params) => {
      calls.push([method, params]);
      if (method === 'core.info') return { canvas: { width: 1280, height: 720 } };
      if (method === 'source.add') return { id: 'lower-third', type: params.type };
      return method === 'plugin.list' ? { plugins: [] } : method === 'device.discover' ? { candidates: [] } : method === 'media.list' ? { items: [] } : {};
    },
    onRender() { return () => {}; },
    want() { return { update() {}, release() {} }; },
    sheet: { attach() { return () => {}; } },
  };
  const added = [];
  const scenes = { itemAdd: async (...args) => added.push(args), reread: async () => {}, undo: { record() {} } };
  const dialog = await openSceneSources(client, scenes, { id: 'wide', name: 'Wide', sources: [] });
  const quick = [...dialog.el.querySelectorAll('button')].find(b => b.textContent === 'Add text');
  test('Add text is offered beside the search box', () => ok(quick));
  quick.click();
  await new Promise(r => setTimeout(r, 50));
  test('Add text adds a text source and places it as a lower third', () => {
    const add = calls.find(([m]) => m === 'source.add');
    ok(add && add[1].type === 'text/source', JSON.stringify(calls));
    eq(added.length, 1);
    const extra = added[0][2] || {};
    ok(extra.transform && extra.transform.frame.w === Math.round(1280 * 0.6), JSON.stringify(added[0]));
  });
}
