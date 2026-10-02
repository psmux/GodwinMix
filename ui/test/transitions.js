// The transition picker beside Take and an item's Enter and Exit, against the
// request shapes the core accepts.
export async function transitionTests(test, eq, ok) {
  const KEY = 'gmx.studio.take';
  let before = null;
  try { before = localStorage.getItem(KEY); localStorage.removeItem(KEY); } catch { /* private window */ }
  const { picker, TYPES } = await import('../panels/multiview/transition-picker.js');

  const p = picker();
  const [type, option, easing, length] = p.el.querySelectorAll('select');
  test('a fresh picker asks for a plain fade, as Take always has', () => {
    eq(p.request(500), { type: 'fade', duration_ms: 500 });
    ok(option.hidden, 'a fade has no direction to choose');
  });
  test('every built in transition is in the list', () => {
    eq([...type.options].map(o => o.value), TYPES.map(t => t.type));
  });

  const pick = (select, value) => { select.value = value; select.dispatchEvent(new Event('change')); };
  pick(type, 'wipe');
  pick(option, 'up');
  test('a wipe carries its direction', () => {
    ok(!option.hidden, 'the direction list shows for a wipe');
    eq(p.request(1000), { type: 'wipe', duration_ms: 1000, params: { direction: 'up' } });
    eq(p.describe(), 'Wipe up 0.5 s');
  });
  pick(type, 'dip');
  test('a dip offers colours in place of directions', () => {
    eq([...option.options].map(o => o.value), ['black', 'white']);
    pick(option, 'white');
    eq(p.request(500).params, { colour: 'white' });
  });
  pick(easing, 'linear');
  pick(length, '1000');
  test('the easing goes in params and the length is the one chosen', () => {
    eq(p.request(p.ms()), { type: 'dip', duration_ms: 1000, params: { colour: 'white', easing: 'linear' } });
  });
  test('the choice is remembered for the next page', () => {
    const again = picker();
    eq(again.request(again.ms()), { type: 'dip', duration_ms: 1000, params: { colour: 'white', easing: 'linear' } });
  });

  const loaded = picker();
  await loaded.load({ call: async () => ({ transitions: [
    { name: 'fade', origin: 'built-in', type: 'fade' },
    { name: 'house', origin: 'collection', type: 'fade', duration_ms: 400 },
  ] }) });
  test('the collection\'s own transitions join the list', () => {
    ok([...loaded.el.querySelector('select').options].some(o => o.value === 'house'));
  });
  try { if (before === null) localStorage.removeItem(KEY); else localStorage.setItem(KEY, before); } catch { /* ignore */ }

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
