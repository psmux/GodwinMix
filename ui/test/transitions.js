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

  // The core's default (fx.assign, kept in fx-assign.json) is what a page
  // nobody picked on shows and what its Take names.
  try { for (const k of KEYS) localStorage.removeItem(k); } catch { /* private window */ }
  const { follow, assignedFor } = await import('../panels/multiview/transition-default.js');
  const CATALOGUE = [
    { name: 'film-burn', origin: 'fx', type: 'shader', duration_ms: 1500 },
    { name: 'light-leak', origin: 'fx', type: 'overlay', duration_ms: 1200 },
  ];
  const ASSIGNED = { default: 'film-burn', scenes: { Interview: 'light-leak' } };
  test('a scene\'s own assignment wins over the default, as the core takes it', () => {
    eq(assignedFor(ASSIGNED, 'Interview'), 'light-leak');
    eq(assignedFor(ASSIGNED, 'Scene 4'), 'film-burn');
    eq(assignedFor({}, 'Scene 4'), null);
  });
  const d = new TransitionState();
  d.learn(CATALOGUE);
  follow(d, ASSIGNED, 'Scene 4');
  test('nothing picked here: the core\'s default is shown and Take names it at its own length', () => {
    ok(!d.picked, 'a fresh page has picked nothing');
    eq(d.describe(), 'film-burn 1.5 s');
    eq(d.request(d.ms()), { type: 'film-burn', duration_ms: 1500 });
  });
  follow(d, { default: 'wipe' }, null);
  test('a built in default runs at the protocol\'s default length with no params', () => {
    eq(d.describe(), 'Wipe left 0.3 s');
    eq(d.request(d.ms()), { type: 'wipe', duration_ms: 300 });
  });
  follow(d, {}, null);
  test('with nothing assigned a fresh page still takes with a plain fade', () => {
    eq(d.request(d.ms()), { type: 'fade', duration_ms: 500 });
  });
  d.set({ type: 'push' });
  follow(d, ASSIGNED, 'Scene 4');
  test('once the person picks one, the page keeps it over the core\'s default', () => {
    ok(d.picked);
    eq(d.request(d.ms()), { type: 'push', duration_ms: 500, params: { direction: 'left' } });
  });

  try { for (const k of KEYS) localStorage.removeItem(k); } catch { /* private window */ }
  const { picker } = await import('../panels/multiview/transition-picker.js');
  let armed = 'Scene 4';
  const stub = {
    call: async (method) => method === 'program.transitions'
      ? { transitions: CATALOGUE, default_duration_ms: 300 }
      : method === 'fx.list' ? { fx: [], assigned: ASSIGNED } : {},
    on: () => () => {},
  };
  const tp = picker({ client: stub, armedScene: () => armed });
  await tp.load(stub);
  test('the control beside Take shows the core\'s default and Take sends what it shows', () => {
    ok(tp.el.textContent.includes('film-burn') && tp.el.textContent.includes('1.5 s'), tp.el.textContent);
    ok(!tp.el.textContent.includes('Fade'), 'no fade is shown while film-burn is what a take runs');
    eq(tp.request(tp.ms()), { type: 'film-burn', duration_ms: 1500 });
  });
  armed = 'Interview';
  test('the armed scene\'s own transition is the one Take names', () => {
    eq(tp.request(tp.ms()), { type: 'light-leak', duration_ms: 1200 });
    eq(tp.describe(), 'light-leak 1.2 s');
  });
  tp.choose('fade');
  test('a pick on this page is shown and sent from then on', () => {
    ok(tp.el.textContent.includes('Fade'), tp.el.textContent);
    eq(tp.request(tp.ms()), { type: 'fade', duration_ms: 500 });
  });
  tp.close();
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
