// Each show's picture, for the rows on screen only, every two seconds,
// only while the wall is open and the tab is visible. A picture costs the
// station a keyframe decode, so a row scrolled away stops asking at once.
//
// One <img> per show is kept and moved into whichever row draws it, so a
// redraw never blanks a picture, and the next one is loaded off screen and
// swapped in when it has arrived.

const EVERY_MS = 2000;

/** The picture's address: the client's own builder, or the station's route. */
export function thumbUrl(client, id, width, t) {
  if (client.showThumbUrl) return client.showThumbUrl(id, width, t);
  const tr = client.transport || {};
  const u = new URL(`/api/v1/shows/${encodeURIComponent(id)}/thumbnail.jpg`, tr.base || location.origin);
  u.searchParams.set("width", String(width));
  u.searchParams.set("t", String(t));
  if (tr.token) u.searchParams.set("token", tr.token);
  return u.toString();
}

export class Thumbs {
  constructor(client, width = 160) {
    this.client = client;
    this.width = width;
    this.imgs = new Map();
    this.ids = [];
    this.asked = 0;
    this.timer = setInterval(() => this.refresh(), EVERY_MS);
  }

  /** The <img> for one show; created blank the first time. */
  img(id) {
    let img = this.imgs.get(id);
    if (!img) {
      img = document.createElement("img");
      img.className = "wl-pic";
      img.alt = "";
      img.decoding = "async";
      img.dataset.empty = "1";
      this.imgs.set(id, img);
    }
    return img;
  }

  /** The shows on screen. Any new one is asked for now. */
  visible(ids) {
    const fresh = ids.filter((id) => !this.imgs.has(id) || this.imgs.get(id).dataset.empty === "1");
    this.ids = ids;
    for (const id of fresh) this.load(id);
  }

  refresh() {
    if (document.hidden) return;
    for (const id of this.ids) this.load(id);
  }

  load(id) {
    const img = this.img(id);
    if (img.dataset.loading === "1") return;
    img.dataset.loading = "1";
    this.asked += 1;
    const next = new Image();
    next.decoding = "async";
    const done = (ok) => {
      img.dataset.loading = "0";
      if (!ok) return;
      img.src = next.src;
      img.dataset.empty = "0";
    };
    next.onload = () => done(true);
    next.onerror = () => done(false);
    next.src = thumbUrl(this.client, id, this.width, Math.floor(Date.now() / EVERY_MS));
  }

  stop() {
    clearInterval(this.timer);
    this.ids = [];
    for (const img of this.imgs.values()) img.removeAttribute("src");
    this.imgs.clear();
  }
}
