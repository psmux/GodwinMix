// Whether the programme monitor should hold a mosaic subscription.
//
// Not conditional on a programme cell being known. The cell is only reported
// once the mosaic is built, and the mosaic is only built while something is
// subscribed, so waiting for the cell before subscribing meant waiting for
// ever whenever nothing else on the page wanted the mosaic. With a gallery of
// icons, which is what the church preset ships, nothing else did, and the
// monitor said "Preview paused while hidden" over a live programme.
//
// Its own module, with nothing imported, so the tests can reach it without
// pulling the panel and the shell in behind it.

export function mosaicWanted(state, visible) {
  return !!(visible && !document.hidden && state.multiview && state.multiview.enabled);
}
