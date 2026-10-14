// Paint a monitor on the screen's next refresh, once, however many times it
// was asked for since the last one.
//
// Chromium finishes a 2D canvas at the end of every task that drew on it. A
// canvas drawn in two tasks between two refreshes has got ahead of the
// compositor, and Chromium then holds the page until the GPU has done all it
// was given (SharedContextRateLimiter, a glFinish) before it runs the next
// task. Each decoded mosaic frame used to be drawn in its own task, and the
// Sources panel drew its tiles again on every render, so the page waited on
// the GPU over and over. On 2026-10-14, on a laptop whose GPU was also busy
// with a screen capture and a browser source, each wait was 100 to 400 ms: the
// mixer sent 25 frames a second, the page read about 3 of them, and the
// monitors stopped, moved and stopped again. Drawing in requestAnimationFrame
// puts every draw into the frame the compositor is about to take, so a canvas
// never gets ahead and the page never waits.

/**
 * `paint` on the next animation frame. `request()` any number of times; it
 * runs once. `pending` says a paint is waiting, so a caller can skip decoding
 * a frame nobody will see before the next one arrives.
 */
export function nextFrame(paint) {
  const schedule = typeof requestAnimationFrame === "function"
    ? (fn) => requestAnimationFrame(fn)
    : (fn) => setTimeout(fn, 16);
  const cancel = typeof cancelAnimationFrame === "function" ? cancelAnimationFrame : clearTimeout;
  let id = 0;
  return {
    get pending() {
      return id !== 0;
    },
    request() {
      if (id) return;
      id = schedule(() => {
        id = 0;
        paint();
      });
    },
    cancel() {
      if (id) cancel(id);
      id = 0;
    },
  };
}
