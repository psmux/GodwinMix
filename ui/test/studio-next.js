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
    eq(nextUp(panel, {}, kit(null), null), { kind: 'scene', id: 'wide', name: 'Wide', why: 'next' });
  });
  test('an armed scene that is not on air is what Preview holds', () => {
    eq(nextUp(panel, { preview: 'Two shot', scene: 'Wide' }, kit('wide'), null), { kind: 'scene', id: 'two', name: 'Two shot', why: 'armed' });
  });
  // Somebody else takes Two shot while Wide is on air: an agent, another desk.
  follow(panel, { scene: 'Wide' }, kit('wide'));
  follow(panel, { scene: 'Two shot', preview: 'Two shot' }, kit('two'));
  test('a take from anywhere leaves Preview on where the show came from, not on black or the same shot twice', () => {
    const next = nextUp(panel, { scene: 'Two shot', preview: 'Two shot' }, kit('two'), null);
    eq(next, { kind: 'scene', id: 'wide', name: 'Wide', why: 'before' });
    eq(whyText(next), 'Suggested: on air before this');
  });
  test('a source armed here is held until it goes on air itself', () => {
    const s = { scene: 'Two shot', sources: [{ id: 'cam-wide', name: 'Wide camera' }] };
    eq(nextUp(panel, s, kit('two'), 'cam-wide').kind, 'source');
    eq(nextUp(panel, { program: 'cam-wide', scene: null }, kit(null), 'cam-wide').kind, 'scene');
  });
  test('with no scenes at all there is nothing to suggest', () => {
    const empty = { scenes: () => [], summary: () => null, live: () => null, armed: () => null };
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
    eq(sourceTrouble({ sources }, 'gone'), 'not in this mixer');
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
