// The transition beside Take and an item's Enter and Exit, against the
// request shapes the core accepts.
export async function transitionTests(test, eq, ok) {
  const KEYS = ['gmx.studio.take', 'gmx.studio.used'];
  const before = {};
  try { for (const k of KEYS) { before[k] = localStorage.getItem(k); localStorage.removeItem(k); } } catch { /* private window */ }
  const { TransitionState, TYPES, FIRST_PICKS } = await import('../panels/multiview/transition-state.js');

  const p = new TransitionState();
  test('a fresh choice is a plain fade, as Take always has been', () => {
    eq(p.request(500), { type: 'fade', duration_ms: 500 });
    eq(p.options(), []);
  });
  p.set({ type: 'wipe' });
  test('a wipe starts on its first direction and carries it', () => {
    eq(p.request(500), { type: 'wipe', duration_ms: 500, params: { direction: 'left' } });
    p.set({ option: 'up', ms: 1000 });
    eq(p.request(p.ms()), { type: 'wipe', duration_ms: 1000, params: { direction: 'up' } });
    eq(p.describe(), 'Wipe up 1 s');
  });
  p.set({ type: 'dip' });
  test('a dip offers colours in place of directions', () => {
    eq(p.options().map(([v]) => v), ['black', 'white']);
    p.set({ option: 'white' });
    eq(p.request(500).params, { colour: 'white' });
  });
  p.set({ easing: 'linear' });
  test('the easing goes in params and the length is the one chosen', () => {
    eq(p.request(p.ms()), { type: 'dip', duration_ms: 1000, params: { colour: 'white', easing: 'linear' } });
  });
  test('the choice is remembered for the next page', () => {
    const again = new TransitionState();
    eq(again.request(again.ms()), { type: 'dip', duration_ms: 1000, params: { colour: 'white', easing: 'linear' } });
  });

  const q = new TransitionState();
  test('the quick picks start as fade, wipe and dip', () => eq(q.picks(), FIRST_PICKS));
  q.set({ type: 'push' });
  q.used();
  q.used();
  q.set({ type: 'zoom' });
  q.used();
  test('the quick picks follow what this desk takes with, three at most', () => {
    eq(q.picks(), ['push', 'zoom', 'fade']);
  });
  q.learn([
    { name: 'fade', origin: 'built-in', type: 'fade' },
    { name: 'house', origin: 'collection', type: 'fade', duration_ms: 400 },
    { name: 'light-leak', origin: 'fx', type: 'overlay', duration_ms: 1500 },
  ]);
  test('the collection\'s and the fx library\'s transitions are learned, not the built in ones twice', () => {
    eq([...q.extra.keys()], ['house', 'light-leak']);
    ok(TYPES.length === 9, 'the built in list changed');
  });
  q.set({ type: 'light-leak' });
  test('a clip from the fx library runs for its own length', () => {
    eq(q.describe(), 'light-leak 1.5 s');
    eq(q.request(500), { type: 'light-leak', duration_ms: 500, params: { easing: 'linear' } });
  });
  try { for (const k of KEYS) { if (before[k] === null) localStorage.removeItem(k); else localStorage.setItem(k, before[k]); } } catch { /* ignore */ }

  const { takeBarTests } = await import('./take-bar.js');
  await takeBarTests(test, eq, ok);

  const pick = (select, value) => { select.value = value; select.dispatchEvent(new Event('change')); };
  const { motionValue, motionSection } = await import('../panels/composer/motion.js');
  test('an item motion is stored the way the document reads it', () => {
    eq(motionValue('slide', 'bottom', '500', 'ease-out', false), { type: 'slide', duration_ms: 500, easing: 'ease-out', edge: 'bottom' });
    eq(motionValue('fade', 'left', 300, 'linear', true), { type: 'fade', duration_ms: 300, easing: 'linear', on_take: true });
    eq(motionValue('', 'left', 300, 'linear', false), null);
  });

  const calls = [];
  const scenes = { itemSet: async (scene, item, props, opts) => { calls.push({ scene, item, props, draft: opts.draft || null }); return {}; } };
  const section = motionSection({ id: 'lower-third', visible: true }, {
    scenes,
    context: () => ({ scene: 'show', draft: 'd1', items: ['lower-third'] }),
    changed: () => {},
  });
  const [enterKind, enterEdge] = section.querySelectorAll('select');
  pick(enterKind, 'slide');
  pick(enterEdge, 'right');
  await new Promise(r => setTimeout(r, 0));
  test('choosing an Enter writes it to the item on the draft being edited', () => {
    const last = calls[calls.length - 1];
    eq(last.props, { enter: { type: 'slide', duration_ms: 300, easing: 'ease-out', edge: 'right' } });
    eq(last.draft, 'd1');
  });
  calls.length = 0;
  [...section.querySelectorAll('button')].find(b => b.textContent === 'Hide on air').click();
  await new Promise(r => setTimeout(r, 0));
  test('Hide on air hides it on the scene itself, then on the draft', () => {
    eq(calls.map(c => [c.props.visible, c.draft]), [[false, null], [false, 'd1']]);
  });
}
