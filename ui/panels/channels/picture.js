// A live channel stream's picture: GET /api/v1/channels/{id}/streams/{name}/
// thumbnail.jpg. The listener decodes keyframes only while somebody asks,
// and stops ten seconds after the last ask, so a picture is asked for only
// while it is on screen: the Channels panel's tick, or the wall's.
//
// The next picture is loaded off screen and swapped in when it has arrived,
// so a slow answer never blanks the one showing.

/** The picture's address, with the token for an <img> that cannot send a header. */
export function channelThumbUrl(client, id, stream, width, t) {
  const tr = (client && client.transport) || {};
  const path = `/api/v1/channels/${encodeURIComponent(id)}/streams/${encodeURIComponent(stream)}/thumbnail.jpg`;
  const u = new URL(path, tr.base || location.origin);
  u.searchParams.set("width", String(width));
  u.searchParams.set("t", String(t));
  if (tr.token) u.searchParams.set("token", tr.token);
  return u.toString();
}

/**
 * One <img> that keeps its last picture while the next loads.
 * `img.dataset.empty` is "1" until a picture has arrived.
 */
export function streamPicture(client, className, width = 320) {
  const img = document.createElement("img");
  img.className = className;
  img.alt = "";
  img.decoding = "async";
  img.dataset.empty = "1";
  let loading = false;
  return {
    node: img,
    /** Ask for a picture of `stream` on channel `id` now. */
    load(id, stream) {
      if (loading) return;
      loading = true;
      const next = new Image();
      next.decoding = "async";
      next.onload = () => {
        loading = false;
        img.src = next.src;
        img.dataset.empty = "0";
      };
      next.onerror = () => { loading = false; };
      next.src = channelThumbUrl(client, id, stream, width, Date.now());
    },
    /** Nothing is live: the last picture goes, so it cannot say otherwise. */
    blank() {
      if (img.dataset.empty === "1") return;
      img.removeAttribute("src");
      img.dataset.empty = "1";
    },
  };
}
