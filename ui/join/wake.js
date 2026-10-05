// Keep the screen on while publishing. A phone that dims and locks stops its
// camera, and the mixer's source freezes on its last picture.
//
// A screen wake lock is let go by the browser whenever the page is hidden, so
// it is asked for again each time the page comes back while still live. Not
// every browser has one (Firefox on Android, older Safari); there the page
// says to keep the phone awake by hand, which is in the docs.

export class ScreenAwake {
  /** `nav` is swappable so the tests need no real lock. */
  constructor(nav = typeof navigator !== "undefined" ? navigator : {}, doc = typeof document !== "undefined" ? document : null) {
    this.nav = nav;
    this.doc = doc;
    this.wanted = false;
    this.lock = null;
    this.onVisible = () => {
      if (this.wanted && !this.lock && this.doc && this.doc.visibilityState === "visible") this.take();
    };
    if (this.doc) this.doc.addEventListener("visibilitychange", this.onVisible);
  }

  /** Can this browser keep the screen on at all? */
  get possible() {
    return !!(this.nav.wakeLock && this.nav.wakeLock.request);
  }

  /** Keep the screen on (true) or let it sleep again (false). */
  want(on) {
    this.wanted = !!on;
    if (on && !this.lock) return this.take();
    if (!on) this.letGo();
    return Promise.resolve();
  }

  async take() {
    // One request at a time: every state change asks, and a lock that is
    // still on its way would otherwise be asked for twice.
    if (!this.possible || this.asking) return;
    this.asking = true;
    try {
      const lock = await this.nav.wakeLock.request("screen");
      if (!this.wanted) return void lock.release().catch(() => {});
      this.lock = lock;
      lock.addEventListener?.("release", () => {
        if (this.lock === lock) this.lock = null;
      });
    } catch {
      // Refused: the page is hidden, or the battery saver is on. The next
      // visibilitychange asks again.
    } finally {
      this.asking = false;
    }
  }

  letGo() {
    const lock = this.lock;
    this.lock = null;
    if (lock) lock.release().catch(() => {});
  }

  destroy() {
    this.want(false);
    if (this.doc) this.doc.removeEventListener("visibilitychange", this.onVisible);
  }
}
