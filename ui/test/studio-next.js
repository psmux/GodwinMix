// What Studio mode's preview holds when nothing is armed, what it says over a
// source with no picture, and Studio mode as the starting layout.
export async function studioNextTests(test, eq, ok) {
  const { follow, nextUp, whyText } = await import('../panels/multiview/studio-next.js');
  const { paintSlots, sourceTrouble } = await import('../panels/multiview/studio-slots.js');
  const scenes = [{ id: 'wide', name: 'Wide' }, { id: 'two', name: 'Two shot' }, { id: 'cam', name: 'Camera' }];
  const kit = (live, armed = null) => ({
    scenes: () => scenes,
    summary: (x) => scenes.find((s) => s.id === x || s.name === x) || null,
    live: () => live,
    armed: () => armed,
    view: (id) => id === 'two' ? { canvas: { width: 1920, height: 1080 }, geometry: [
      { item: 'a', source: 'cam-wide', x: 0, y: 0, width: 960, height: 1080 },
      { item: 'b', source: 'cam-late', x: 960, y: 0, width: 960, height: 1080 },
    ] } : { canvas: { width: 1920, height: 1080 }, geometry: [{ item: 'w', source: 'cam-wide', x: 0, y: 0, width: 1920, height: 1080 }] },
    mirror: { record: () => ({ visible: true }) },
  });
  const sources = [{ id: 'cam-wide', name: 'Wide camera', state: 'live' }, { id: 'cam-late', name: 'Late camera', state: 'connecting' }];

  const panel = {};
  test('with nothing armed and nothing on air, Preview suggests the first scene', () => {
    follow(panel, {}, kit(null));
    eq(nextUp(panel, {}, kit(null), null, null, 'swap'), { kind: 'scene', id: 'wide', name: 'Wide', why: 'next' });
  });
  test('an armed scene that is not on air is what Preview holds', () => {
    eq(nextUp(panel, { preview: 'Two shot', scene: 'Wide' }, kit('wide'), null, null, 'swap'), { kind: 'scene', id: 'two', name: 'Two shot', why: 'armed' });
  });
  // Somebody else takes Two shot while Wide is on air: an agent, another desk.
  follow(panel, { scene: 'Wide' }, kit('wide'));
  follow(panel, { scene: 'Two shot', preview: 'Two shot' }, kit('two'));
  test('with After Take set to swap, a take leaves Preview on where the show came from', () => {
    const next = nextUp(panel, { scene: 'Two shot', preview: 'Two shot' }, kit('two'), null, null, 'swap');
    eq(next, { kind: 'scene', id: 'wide', name: 'Wide', why: 'before' });
    eq(whyText(next), 'Suggested: on air before this');
  });
  // Picked in the Scenes panel while it is the one on air: the person asked
  // for it, so Preview holds it and Take sends it, rather than a suggestion.
  test('the scene on air, picked on this page, is what Preview holds', () => {
    const s = { scene: 'Two shot', preview: 'Two shot' };
    eq(nextUp(panel, s, kit('two'), null, 'two', 'swap'), { kind: 'scene', id: 'two', name: 'Two shot', why: 'armed' });
    eq(nextUp(panel, s, kit('two'), null, 'wide', 'swap').why, 'before', 'a pick of another scene says nothing about this one');
  });
  // The default since the owner's ruling: Preview keeps what was just taken.
  test('by default a take leaves Preview on the scene just taken, as Programme shows it', () => {
    eq(nextUp(panel, { scene: 'Two shot', preview: 'Two shot' }, kit('two'), null), { kind: 'scene', id: 'two', name: 'Two shot', why: 'armed' });
    eq(nextUp(panel, { scene: 'Two shot' }, kit('two'), null), { kind: 'scene', id: 'two', name: 'Two shot', why: 'live' });
    eq(whyText({ kind: 'scene', id: 'two', name: 'Two shot', why: 'live' }), '');
  });
  test('by default a source taken on its own stays in Preview, and nothing on air shows the first scene', () => {
    eq(nextUp(panel, { program: 'cam-wide', scene: null, sources: [{ id: 'cam-wide', name: 'Wide camera' }] }, kit(null), null), { kind: 'source', id: 'cam-wide', name: 'Wide camera', why: 'live' });
    eq(nextUp(panel, {}, kit(null), null).id, 'wide');
  });
  const { resolve } = await import('../panels/multiview/studio-armed.js');
  test('a take forgets the pick made on this page', () => {
    const k = Object.assign(kit('two'), { supported: true });
    const pane = { sceneSession: { scenes: k } };
    document.body.dataset.chosen = 'two';
    eq(resolve(pane, { scene: 'Two shot', preview: 'Two shot' }).why, 'armed');
    k.live = () => 'wide';
    const next = resolve(pane, { scene: 'Wide', preview: 'Two shot' });
    eq(document.body.dataset.chosen, undefined, 'the pick outlived the take');
    eq(next, { kind: 'scene', id: 'two', name: 'Two shot', why: 'armed' }, 'what is armed and not on air is still held');
    delete document.body.dataset.chosen;
  });
  // The core arms a scene taken by name (scene/server/after_take.rs), once,
  // for every page. A page that armed it as well raced the other operators.
  test('a take by name arms nothing from the page; the core does it', () => {
    const k = Object.assign(kit('wide'), { supported: true });
    const asked = [];
    const pane = { sceneSession: { scenes: k }, client: { call: async (m, p) => { asked.push([m, p]); } } };
    resolve(pane, { scene: 'Wide', preview: 'Two shot' });
    k.live = () => 'cam';
    resolve(pane, { scene: 'Camera', preview: 'Two shot' });
    eq(asked, [], 'a Stream Deck took Camera by name');
  });
  test('a source armed here and taken is forgotten once something else goes on air', () => {
    const k = Object.assign(kit(null), { supported: true });
    const pane = { sceneSession: { scenes: k } };
    document.body.dataset.armed = 'cam-wide';
    eq(resolve(pane, { program: 'cam-wide', scene: null }).kind, 'source', 'the camera taken stays in Preview');
    k.live = () => 'two';
    resolve(pane, { program: 'cam-wide', scene: 'Two shot' });
    eq(document.body.dataset.armed, undefined, 'a Stream Deck took Two shot and the camera stayed armed');
    document.body.dataset.armed = 'cam-late';
    k.live = () => 'wide';
    resolve(pane, { scene: 'Wide' });
    eq(document.body.dataset.armed, 'cam-late', 'a camera armed to go next is kept through a take of something else');
    delete document.body.dataset.armed;
  });
  test('a source armed here is held until it goes on air itself', () => {
    const s = { scene: 'Two shot', sources: [{ id: 'cam-wide', name: 'Wide camera' }] };
    eq(nextUp(panel, s, kit('two'), 'cam-wide', null, 'swap').kind, 'source');
    eq(nextUp(panel, { program: 'cam-wide', scene: null }, kit(null), 'cam-wide', null, 'swap').kind, 'scene');
    eq(nextUp(panel, { program: 'cam-wide', scene: null }, kit(null), 'cam-wide').kind, 'source', 'by default it stays once taken');
  });
  test('with no scenes at all there is nothing to suggest', () => {
    const empty = { scenes: () => [], summary: () => null, live: () => null, armed: () => null };
    eq(nextUp({}, {}, empty, null, null, 'swap'), null);
    eq(nextUp({}, {}, empty, null), null);
  });

  const layer = document.createElement('div');
  const host = { previewSlots: layer };
  paintSlots(host, { sources, multiview: { enabled: true } }, kit('wide'), { kind: 'scene', id: 'two', name: 'Two shot', why: 'before' });
  test('a source with no picture yet shows its name and state on its own box', () => {
    const slots = layer.querySelectorAll('.preview-slot');
    eq(slots.length, 1);
    ok(slots[0].textContent.includes('Late camera') && slots[0].textContent.includes('connecting'), slots[0].textContent);
    eq(slots[0].style.left, '50%');
  });
  paintSlots(host, { sources: [], multiview: { enabled: true } }, kit(null), null);
  test('an empty preview says what to do rather than showing black', () => {
    ok(layer.textContent.includes('Nothing to preview yet'), layer.textContent);
  });
  test('a source the mixer does not have, or one that failed, says so', () => {
    eq(sourceTrouble({ sources }, 'gone'), 'removed, left out of the take');
    eq(sourceTrouble({ sources: [{ id: 'x', state: 'failed' }] }, 'x'), 'failed, trying again');
    eq(sourceTrouble({ sources }, 'cam-wide'), null);
  });
  test('a clip held on its last frame has a picture, so its box carries no words', () => {
    eq(sourceTrouble({ sources: [{ id: 'clip', state: 'live', ended: true, has_video: true }] }, 'clip'), null);
  });

  // Take with nothing armed sends the suggestion, by id, with the transition.
  const studio = await import('../panels/multiview/studio.js');
  const calls = [];
  const pane = { client: { state: {}, call: async (method, params) => calls.push({ method, params }) } };
  pane.next = { kind: 'scene', id: 'wide', name: 'Wide', why: 'before' };
  await studio.take(pane, 500);
  test('Take with nothing armed takes the suggested scene', () => {
    eq(calls[0], { method: 'program.take', params: { scene: 'wide', transition: { type: 'fade', duration_ms: 500 } } });
  });

  // Studio mode is where a new browser starts, and where one that never
  // chose starts too; a choice somebody made is kept.
  const KEY = 'gmx.settings';
  let saved = null;
  try { saved = localStorage.getItem(KEY); } catch { /* private window */ }
  const fresh = async (value) => {
    try { if (value === null) localStorage.removeItem(KEY); else localStorage.setItem(KEY, JSON.stringify(value)); } catch { /* ignore */ }
    const m = await import(`../shell/settings.js?case=${Math.random()}`);
    return m;
  };
  let m = await fresh(null);
  test('a new browser starts in Studio mode', () => eq(m.settings().producer, true));
  m = await fresh({ producer: false, gallery: 'icon' });
  test('a stored false nobody chose (every setting used to be saved at once) starts in Studio mode', () => {
    eq(m.settings().producer, true);
    eq(m.settings().gallery, 'icon');
  });
  m.setSetting('producer', false);
  m = await fresh(JSON.parse(localStorage.getItem(KEY)));
  test('turning Studio mode off is remembered', () => eq(m.settings().producer, false));
  try { if (saved === null) localStorage.removeItem(KEY); else localStorage.setItem(KEY, saved); } catch { /* ignore */ }
}
