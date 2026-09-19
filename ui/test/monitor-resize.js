import { settings, setSetting } from '../shell/settings.js';

/** Real canvas geometry, without asking the media pipeline to rebuild in a test. */
export async function monitorResizeTests(test, eq, ok, ProgramPanel) {
  const panel = new ProgramPanel(), host = document.createElement('div');
  const producer = settings().producer;
  const updates = [], released = [];
  const canvas = () => {
    const node = document.createElement('canvas');
    node.style.width = '400px'; host.append(node); return node;
  };
  panel.canvas = canvas(); panel.previewCanvas = canvas(); panel.note = document.createElement('div');
  document.body.append(host);
  panel.visible = true;
  panel.setClient({
    state: { multiview: { enabled: true, cols: 1, cells: [{ source: null, index: 0 }] }, preview: 'scene-wide' },
    sheet: { attach: () => () => {} }, preview: { attach: () => () => {} },
    want: kind => ({ update: value => updates.push({ kind, ...value }), release: () => released.push(kind) }),
  });
  try {
    setSetting('producer', true);
    panel.retune();
    const originalWidth = panel.canvas.width;
    for (const width of [440, 520, 600, 700]) {
      panel.canvas.style.width = panel.previewCanvas.style.width = width + 'px';
      panel.scheduleRetune();
      // State flushes during dragging must not bypass the resize quiet period.
      panel.retune();
      await new Promise(resolve => setTimeout(resolve, 20));
    }
    test('monitor resize keeps stream sizes and backing stores stable during dragging', () => {
      eq(updates.length, 0); eq(panel.canvas.width, originalWidth);
      eq(panel.canvas.getBoundingClientRect().width, 700);
    });
    await new Promise(resolve => setTimeout(resolve, 180));
    test('settled monitor resize updates each stream once and restores sharp canvas sizes', () => {
      eq(updates.map(x => x.kind).sort(), ['multiview', 'preview']);
      eq(panel.canvas.width, Math.round(700 * Math.min(devicePixelRatio || 1, 2)));
      eq(panel.previewCanvas.width, panel.canvas.width);
    });
    panel.scheduleRetune();
    setSetting('producer', false); panel.retune();
    test('switching studio mode off releases preview without waiting for resize', () => {
      eq(released, ['preview']); ok(panel.previewWant === null);
    });
    setSetting('producer', true); panel.retune();
    panel.client.state.preview = null;
    panel.retune();
    test('preview changes release the stream immediately during a resize', () => {
      eq(released, ['preview', 'preview']); ok(panel.previewWant === null);
    });
    panel.client.state.preview = 'scene-close'; panel.retune();
    panel.setWorkspaceActive(false);
    const before = updates.length;
    await new Promise(resolve => setTimeout(resolve, 180));
    test('hiding a resizing monitor cancels pending work and releases both streams', () => {
      ok(panel.resizeTimer === null && panel.want === null && panel.previewWant === null);
      eq(updates.length, before);
      eq(released, ['preview', 'preview', 'multiview', 'preview']);
    });
  } finally { panel.release(); host.remove(); setSetting('producer', producer); }
}
