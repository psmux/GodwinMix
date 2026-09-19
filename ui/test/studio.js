// Exercise the studio controls against the public request shape.
export async function studioTests(test, eq, ok) {
  window.godwinmixPanels ||= [];
  const { default: ProgramPanel } = await import('../panels/multiview/panel.js');
  const panel = new ProgramPanel();
  const calls = [];
  panel.setClient({ state: { preview: 'wide-scene' }, call: async (method, params) => calls.push({ method, params }) });
  panel.armed = 'wide-scene';
  await panel.take(500);
  test('studio Fade takes a scene with the selected duration inside transition', () => {
    eq(calls[0], { method: 'program.take', params: { scene: 'wide-scene', transition: { type: 'fade', duration_ms: 500 } } });
  });
  panel.client.state.preview = null;
  panel.armed = 'camera';
  await panel.take(0);
  test('studio Cut takes an armed source without a transition', () => {
    eq(calls[1], { method: 'program.take', params: { source: 'camera' } });
  });
  let released = 0;
  panel.want = { release() { released++; } };
  panel.previewWant = { release() { released++; } };
  panel.setWorkspaceActive(false);
  test('a hidden programme panel releases both preview subscriptions', () => {
    eq(released, 2);
    ok(panel.want === null && panel.previewWant === null);
  });
}
