// A game controller as a camera joystick: the left stick points, the right
// stick zooms, A takes Preview to programme and B cuts, through the same
// commands as the Take and Cut buttons. It is read only while camera
// properties or the Preview overlay is open and a controller is plugged in,
// once a frame.

const DEAD = 0.15;
const dz = (v) => (Math.abs(v) < DEAD ? 0 : (v - Math.sign(v) * DEAD) / (1 - DEAD));

/** `on` has {move(pan, tilt), zoom(v), stop(), take(), cut()}. */
export function gamepads(on) {
  let raf = 0;
  let was = { moving: false, zooming: false, buttons: [] };
  const pads = () => (navigator.getGamepads ? [...navigator.getGamepads()].filter(Boolean) : []);

  function read() {
    raf = 0;
    const pad = pads()[0];
    if (!pad) return;
    const [lx = 0, ly = 0, , ry = 0] = pad.axes;
    const pan = dz(lx);
    const tilt = -dz(ly);
    const zoom = -dz(ry);
    const moving = !!(pan || tilt);
    if (moving || was.moving) moving ? on.move(pan, tilt) : on.stop();
    if (zoom || was.zooming) on.zoom(zoom);
    const pressed = pad.buttons.map((b) => b.pressed);
    const down = (i) => pressed[i] && !was.buttons[i];
    if (down(0)) on.take();
    if (down(1)) on.cut();
    was = { moving, zooming: !!zoom, buttons: pressed };
    raf = requestAnimationFrame(read);
  }

  let active = false;
  const start = () => {
    active = true;
    if (!raf && pads().length) raf = requestAnimationFrame(read);
  };
  const connected = () => active && start();
  window.addEventListener("gamepadconnected", connected);
  return {
    start,
    stop() {
      active = false;
      cancelAnimationFrame(raf);
      raf = 0;
      if (was.moving || was.zooming) on.stop();
      was = { moving: false, zooming: false, buttons: [] };
    },
    destroy() {
      this.stop();
      window.removeEventListener("gamepadconnected", connected);
    },
  };
}
