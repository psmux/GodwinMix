// The programme on a display: the frame header, the display list, the width
// asked for, the idle cursor, the picture at its own size, and full screen
// giving its feed back when it closes.
import { parseFrame } from '../client/frames.js';
import { PicturePainter } from '../client/painters.js';
import { describe } from '../panels/programme/displays.js';
import { displayWidth, idleCursor } from '../panels/programme/view.js';

/** A header the way the core writes one, then a few bytes of "JPEG". */
function header(seq, layout) {
  const b = new Uint8Array(20);
  const v = new DataView(b.buffer);
  v.setUint32(0, seq >>> 0, true);
  v.setUint32(4, layout, true);
  v.setBigUint64(8, 1234n, true);
  b.set([0xff, 0xd8, 0xff, 0xd9], 16);
  return b;
}

/** Just enough client for the view: a want to count, and a painter. */
function fakeClient() {
  const asks = [];
  return {
    asks,
    programme: new PicturePainter(),
    want(key, value) {
      const hold = { key, value, released: false };
      asks.push(hold);
      return { update() {}, release() { hold.released = true; } };
    },
  };
}

export async function programmeOutputTests(test, eq, ok) {
  test('a programme frame is a single picture with layout 1, apart from the armed scene', () => {
    const programme = parseFrame(header(0x80000005, 1));
    eq([programme.programme, programme.preview, programme.seq], [true, false, 5]);
    const preview = parseFrame(header(0x80000005, 0));
    eq([preview.programme, preview.preview], [false, true]);
    const mosaic = parseFrame(header(5, 1));
    eq([mosaic.programme, mosaic.preview], [false, false], 'a mosaic frame with layout id 1 is still a mosaic frame');
  });

  test('a display is listed by name and native size, the main one and the last used marked', () => {
    const scr = { label: 'EPSON PJ', width: 1920, height: 1080, devicePixelRatio: 1, left: 1440, top: 0 };
    const d = describe(scr, 1, 'EPSON PJ@1440,0');
    eq(d.text, 'EPSON PJ, 1920 × 1080 (used last time)');
    const laptop = describe({ label: '', width: 1440, height: 900, devicePixelRatio: 2, left: 0, top: 0, isPrimary: true }, 0, null);
    eq(laptop.text, 'Display 1, 2880 × 1800 (main display)');
  });

  test('the feed is asked for at the display width in device pixels, even and bounded', () => {
    eq(displayWidth({ width: 1920 }, 1), 1920);
    eq(displayWidth({ width: 1440 }, 2), 2880, 'the core caps it at the canvas');
    eq(displayWidth({ width: 1366 }, 1.25), 1708);
    eq(displayWidth({ width: 100 }, 1), 320);
  });

  test('the cursor goes after the idle time and comes back on a move', () => {
    const root = document.createElement('div');
    document.body.appendChild(root);
    let due = null;
    const off = idleCursor(root, 2000, { set: (fn) => (due = fn, 1), clear: () => (due = null) });
    ok(!root.classList.contains('idle'));
    due();
    ok(root.classList.contains('idle'), 'idle after the timer');
    document.dispatchEvent(new Event('pointermove'));
    ok(!root.classList.contains('idle'), 'a move brings it back');
    off();
    root.remove();
  });

  const painter = new PicturePainter();
  const canvas = document.createElement('canvas');
  canvas.dataset.natural = '';
  canvas.width = 16;
  canvas.height = 9;
  painter.bitmap = await createImageBitmap(new ImageData(64, 36));
  painter.pictures = 1;
  painter.attach(canvas);
  test('a natural canvas takes the picture size, so the display does the only scale', () => {
    eq([canvas.width, canvas.height], [64, 36]);
  });
  painter.destroy();

  const { toggleFullscreen, isFull } = await import('../panels/programme/fullscreen.js');
  const client = fakeClient();
  toggleFullscreen(client);
  const view = document.querySelector('.programme-full');
  test('full screen puts the programme alone on the page and asks for the feed', () => {
    ok(isFull());
    ok(view && view.querySelector('canvas'), 'a picture and nothing else');
    eq(client.asks.map((a) => [a.key, a.value.fps]), [['programme', 0]]);
  });
  window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }));
  test('Esc leaves full screen and gives the feed back', () => {
    ok(!isFull());
    ok(!document.querySelector('.programme-full'));
    ok(client.asks[0].released, 'the core is told nobody wants it');
    eq(client.programme.targets.size, 0);
  });
  toggleFullscreen(client);
  document.querySelector('.programme-full').dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
  test('a double click leaves as well', () => ok(!isFull() && client.asks[1].released));
}
