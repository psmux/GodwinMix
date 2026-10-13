import { settings, setSetting } from '../shell/settings.js';
import { attachCell, monitorFps, pacer, rateMenu, RATE_CHOICES } from '../panels/multiview/monitor-rate.js';

/** Frames painted out of `count` arriving every `every` ms, with a jitter of up to `jitter` ms. */
function paints(fps, every, count, jitter = 0) {
  let t = 0, n = 0, seed = 7;
  const due = pacer(() => fps, () => t);
  for (let i = 0; i < count; i++) {
    seed = (seed * 48271) % 2147483647;
    t = i * every + (jitter ? (seed % (2 * jitter + 1)) - jitter : 0);
    if (due()) n++;
  }
  return n;
}

/** The frame rate menu in each monitor's title bar, and the cap behind it. */
export async function monitorRateTests(test, eq, ok, ProgramPanel) {
  const before = { ...settings() };
  try {
    test('a cap of 10 paints 10 of every 25 frames a second', () => {
      // Ten seconds of frames, counting the one at the start.
      eq(paints(10, 40, 250), 101);
      eq(paints(5, 1000 / 30, 300), 51);
    });
    test('a stream already at the cap loses no frame to its jitter', () => {
      eq(paints(25, 40, 250, 12), 250);
      eq(paints(10, 100, 100, 30), 100);
    });

    setSetting('multiviewFps', 25);
    const menu = rateMenu('preview', 'Preview');
    test('the menu is named for screen readers and offers Auto then the rates', () => {
      eq(menu.tagName, 'SELECT');
      eq(menu.getAttribute('aria-label'), 'Preview frame rate');
      eq([...menu.options].map(o => o.textContent), ['Auto', '30 fps', '25 fps', '15 fps', '10 fps', '5 fps']);
      eq(RATE_CHOICES[0], 0);
    });
    menu.value = '10';
    menu.dispatchEvent(new Event('change'));
    test('a choice is kept per monitor in this browser', () => {
      eq(JSON.parse(localStorage.getItem('gmx.settings')).previewFpsCap, 10);
      eq(monitorFps('preview'), 10);
      eq(monitorFps('programme'), 25);
      eq(rateMenu('preview', 'Preview').value, '10');
    });
    setSetting('previewFpsCap', 30);
    test('a cap over the Picture rate changes nothing', () => eq(monitorFps('preview'), 25));

    setSetting('programmeFpsCap', 10);
    const observed = [], attached = [];
    const sheet = { observe: fn => (observed.push(fn), () => {}), attach: (c, i) => (attached.push(i), () => {}) };
    attachCell(sheet, document.createElement('canvas'), 3, 'programme');
    attachCell(sheet, document.createElement('canvas'), 4, 'preview');
    test('a capped monitor is painted by a paced observer, an uncapped one as before', () => {
      eq(observed.length, 1); eq(attached, [4]);
    });

    await panelTests(test, eq, ok, ProgramPanel);
  } finally {
    for (const key of ['previewFpsCap', 'programmeFpsCap', 'multiviewFps']) setSetting(key, before[key]);
  }
}

/** What the monitors ask the core for, with a fake client. */
async function panelTests(test, eq, ok, ProgramPanel) {
  const panel = new ProgramPanel(), host = document.createElement('div');
  const wants = new Map();
  const canvas = () => { const c = document.createElement('canvas'); c.style.width = '400px'; host.append(c); return c; };
  panel.canvas = canvas(); panel.previewCanvas = canvas(); panel.note = document.createElement('div');
  document.body.append(host);
  panel.visible = true;
  panel.studio = await import('../panels/multiview/studio.js');
  panel.setClient({
    state: { multiview: { enabled: true, cols: 2, cells: [{ source: null, index: 0 }, { source: 'cam', index: 1 }] }, preview: 'scene-wide' },
    sheet: { attach: () => () => {}, observe: () => () => {} }, preview: { attach: () => () => {} },
    want: (kind, value) => (wants.set(kind, value), { update: v => wants.set(kind, v), release: () => wants.delete(kind) }),
  });
  const producer = settings().producer;
  try {
    setSetting('producer', true);
    setSetting('previewFpsCap', 5); setSetting('programmeFpsCap', 10);
    panel.retune();
    test('an armed scene asks the core for its stream at the Preview cap', () => eq(wants.get('preview').fps, 5));
    test('the mosaic is asked at the Programme cap while Preview has its own stream', () => eq(wants.get('multiview').fps, 10));
    panel.client.state.preview = null;
    document.body.dataset.armed = 'cam';
    setSetting('previewFpsCap', 0);
    panel.retune();
    test('a source in Preview at Auto keeps the mosaic at the Picture rate', () => {
      ok(!wants.has('preview')); eq(wants.get('multiview').fps, 25);
    });
    setSetting('programmeFpsCap', 0);
    panel.retune();
    test('both monitors at Auto ask what they asked before', () => eq(wants.get('multiview').fps, settings().multiviewFps));
  } finally {
    delete document.body.dataset.armed;
    panel.release(); host.remove(); setSetting('producer', producer);
  }
}
