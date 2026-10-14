import { SheetPainter, PicturePainter } from '../client/frames.js';
import { Client } from '../client/index.js';
import { Store } from '../client/store.js';

const refresh = () => new Promise((r) => requestAnimationFrame(() => r()));

/** A 2x2 picture of one colour, standing in for a decoded frame. */
function picture(colour) {
  const c = document.createElement('canvas');
  c.width = 2; c.height = 2;
  const g = c.getContext('2d');
  g.fillStyle = colour; g.fillRect(0, 0, 2, 2);
  return c;
}

function target() {
  const c = document.createElement('canvas');
  c.width = 2; c.height = 2;
  return c;
}

const pixel = (c) => [...c.getContext('2d').getImageData(0, 0, 1, 1).data];

/** Make the next decode hand back `bitmap`, as createImageBitmap would. Returns the undo. */
function decodesTo(bitmap) {
  const real = globalThis.createImageBitmap;
  globalThis.createImageBitmap = () => Promise.resolve(bitmap);
  return () => { globalThis.createImageBitmap = real; };
}

/**
 * The monitors are painted once a screen refresh, from the newest frame, and
 * the page renders only when something changed. Each of these used to make
 * the page wait on the GPU or redraw itself whole, many times a second.
 */
export async function monitorPaintTests(test, eq, ok) {
  const sheet = new SheetPainter();
  sheet.setLayout({ cells: [{ index: 0, x: 0, y: 0, w: 2, h: 2 }] });
  const monitor = target();
  sheet.attach(monitor, 0);
  const frame = { jpeg: new Uint8Array([0xff, 0xd8]) };

  let undo = decodesTo(picture('#ff0000'));
  await sheet.push(frame);
  undo();
  test('a decoded sheet waits for the screen rather than painting in its own task', () => {
    eq(pixel(monitor), [0, 0, 0, 0]);
    ok(sheet.frame.pending, 'no paint was asked for');
  });
  await refresh();
  test('and is painted on the next refresh', () => eq(pixel(monitor), [255, 0, 0, 255]));

  const mark = monitor.getContext('2d');
  mark.fillStyle = '#0000ff'; mark.fillRect(0, 0, 2, 2);
  sheet.attach(monitor, 0);
  test('attaching a canvas again with the picture it holds does not paint it again', () => {
    eq(pixel(monitor), [0, 0, 255, 255]);
  });
  monitor.width = 4;
  sheet.attach(monitor, 0);
  test('a canvas that was cleared by a resize is painted at once', () => eq(pixel(monitor), [255, 0, 0, 255]));

  undo = decodesTo(picture('#00ff00'));
  await sheet.push(frame);
  undo();
  undo = decodesTo(picture('#ffffff'));
  await sheet.push(frame);
  undo();
  test('a frame arriving while a decoded one waits for the screen is let go', () => {
    eq(sheet.sheets, 2, 'the second frame was decoded as well');
  });
  const seen = [];
  sheet.observe((bitmap) => seen.push(bitmap));
  await refresh();
  await refresh();
  test('watchers are handed each sheet once, however many refreshes follow', () => eq(seen.length, 1));
  sheet.destroy();

  const pane = new PicturePainter();
  const preview = target();
  pane.attach(preview);
  undo = decodesTo(picture('#00ff00'));
  await pane.push(frame);
  undo();
  await refresh();
  test('the preview is painted on the refresh after its frame', () => eq(pixel(preview), [0, 255, 0, 255]));
  pane.destroy();

  const client = new Client({ name: 'test' }, new Store());
  let renders = 0;
  client.onRender(() => renders++);
  renders = 0;
  client.handleEvent('meters', { program: [-20, -20], seq: 5 });
  client.handleEvent('flush', { seq: 5 });
  client.handleEvent('meters', { program: [-18, -18], seq: 6 });
  client.handleEvent('flush', { seq: 6 });
  test('a flush after meters alone renders nothing', () => {
    eq(renders, 0);
    eq(client.state.seq, 6, 'the number still moves on');
  });
  client.handleEvent('program.took', { source: 'cam', seq: 7 });
  client.handleEvent('flush', { seq: 7 });
  test('a flush after a change renders once', () => eq(renders, 1));
}
