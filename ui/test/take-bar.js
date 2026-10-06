// The take bar's controls as a person meets them: one transition control,
// three quick picks, a picker that is only built while it is open, and one
// Effects button whatever the size of the fx library.
export async function takeBarTests(test, eq, ok) {
  const KEYS = ['gmx.studio.take', 'gmx.studio.used'];
  const before = {};
  try { for (const k of KEYS) { before[k] = localStorage.getItem(k); localStorage.removeItem(k); } } catch { /* private window */ }
  const { picker } = await import('../panels/multiview/transition-picker.js');
  const calls = [];
  const client = {
    call: async (method, params) => {
      calls.push({ method, params });
      if (method === 'program.transitions') return { transitions: [{ name: 'house', origin: 'collection', type: 'fade' }] };
      if (method === 'fx.list') return { fx: [{ name: 'light-leak', title: 'Light leak', transition: true, kind: 'overlay', duration_ms: 1500, preview: '/api/v1/fx/light-leak/preview.jpg' }], assigned: {} };
      return {};
    },
  };
  const p = picker({ client, armedScene: () => 'Wide' });
  document.body.append(p.el, p.picks, p.sheet);
  await p.load();
  test('the control says what Take will do, and there are three quick picks', () => {
    ok(p.el.textContent.includes('Fade') && p.el.textContent.includes('0.5 s'), p.el.textContent);
    eq([...p.picks.querySelectorAll('button')].map(b => b.getAttribute('aria-label')), ['Fade', 'Wipe', 'Dip']);
  });
  p.picks.querySelectorAll('button')[1].click();
  test('a quick pick changes the next take at once', () => {
    eq(p.request(p.ms()), { type: 'wipe', duration_ms: 500, params: { direction: 'left' } });
    ok(p.el.textContent.includes('Wipe left'), p.el.textContent);
  });
  test('the picker is shut, and empty, until it is opened', () => {
    ok(p.sheet.hidden);
    eq(p.sheet.querySelectorAll('.tp-tile').length, 0);
  });
  p.el.click();
  await new Promise(r => setTimeout(r, 0));
  test('opened, it lists every transition, the built in ones moving', () => {
    ok(!p.sheet.hidden, 'the sheet did not open');
    eq(p.el.getAttribute('aria-expanded'), 'true');
    eq(p.sheet.querySelectorAll('.tp-tile').length, 10);
    eq(p.sheet.querySelectorAll('.tp-moving').length, 9);
    ok(p.sheet.querySelector('.fx-strip'), 'the fx library transition has no moving strip');
  });
  [...p.sheet.querySelectorAll('.tp-tile')].find(t => t.dataset.type === 'push').click();
  [...p.sheet.querySelectorAll('.tp-lengths button')].find(b => b.textContent === '1 s').click();
  test('a tile and a length in the picker set the next take', () => {
    eq(p.request(p.ms()), { type: 'push', duration_ms: 1000, params: { direction: 'left' } });
  });
  document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  test('Escape shuts the picker and nothing in it moves any more', () => {
    ok(p.sheet.hidden);
    eq(p.sheet.querySelectorAll('.tp-moving, .fx-strip').length, 0);
  });
  p.el.remove(); p.picks.remove(); p.sheet.remove();

  const { effects } = await import('../panels/multiview/fx-gallery.js');
  const { get } = await import('../shell/commands.js');
  const fired = [];
  const fxClient = {
    call: async (method, params) => {
      if (method === 'fx.fire') { fired.push(params.name); return {}; }
      return { fx: ['bokeh', 'glitch', 'light-leak', 'film-burn'].map(name => ({ name, title: name })) };
    },
  };
  const fx = effects(fxClient);
  await new Promise(r => setTimeout(r, 0));
  test('four effects are one Effects button, not four buttons beside Take', () => {
    ok(!fx.button.hidden);
    ok(fx.button.textContent.includes('4'), fx.button.textContent);
    eq(fx.sheet.querySelectorAll('.fx-fire').length, 4);
  });
  test('Alt+1 to Alt+9 still play the effects', () => {
    eq(get('fx.fire-1').key, 'Alt+1');
    eq(get('fx.fire-4').key, 'Alt+4');
  });
  await get('fx.fire-2').run();
  fx.sheet.querySelectorAll('.fx-fire')[0].click();
  test('an effect plays with fx.fire, from its key or from the list', () => eq(fired, ['glitch', 'bokeh']));
  fx.off();
  const none = effects({ call: async () => ({ fx: [] }) });
  await new Promise(r => setTimeout(r, 0));
  test('a mixer with no effects shows no Effects button', () => ok(none.button.hidden));
  try { for (const k of KEYS) { if (before[k] === null) localStorage.removeItem(k); else localStorage.setItem(k, before[k]); } } catch { /* ignore */ }
}
