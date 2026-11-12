// A camera's picture in camera properties, and the people the tracker sees
// drawn over it there and over the Preview monitor. The demo paints its
// pictures straight into the canvas. A real camera is a GodwinMix source, so
// its picture is that source's cell of the multiview the monitors already
// receive, which costs the mixer nothing more. With no monitor on the page
// it is the core's snapshot of the source instead, asked for no more often
// than the core serves one ([snapshot] min_interval_secs, a second by
// default), and only while camera properties is open.

const EVERY_MS = 1100;

export function pictures(client) {
  const shots = new Map();

  function fetchShot(cam, width) {
    const now = performance.now();
    const s = shots.get(cam.id) || { img: null, at: -1e9, loading: false };
    shots.set(cam.id, s);
    if (s.loading || now - s.at < EVERY_MS || !client.snapshotUrl) return s;
    const url = client.snapshotUrl(cam.source || cam.id, width);
    if (!url) return s;
    const img = new Image();
    s.loading = true;
    img.onload = () => Object.assign(s, { img, at: performance.now(), loading: false });
    // Refused (too soon, or no picture yet): wait longer before the next ask.
    img.onerror = () => Object.assign(s, { at: performance.now() + 2000, loading: false });
    img.src = url + (url.includes("?") ? "&" : "?") + "t=" + Math.round(now);
    return s;
  }

  return {
    draw(cam, canvas) {
      const ctx = canvas.getContext("2d");
      if (client.paint) return client.paint(cam.id, canvas);
      const sheet = client.sheet;
      const cell = sheet && sheet.bitmap && sheet.layout && (sheet.layout.cells || []).find((c) => c.source === (cam.source || cam.id));
      if (cell) {
        try {
          return ctx.drawImage(sheet.bitmap, cell.x, cell.y, cell.w, cell.h, 0, 0, canvas.width, canvas.height);
        } catch {
          /* a picture closed as the next arrived: the snapshot below, or the next frame */
        }
      }
      const s = fetchShot(cam, canvas.width);
      if (s.img) return ctx.drawImage(s.img, 0, 0, canvas.width, canvas.height);
      ctx.fillStyle = "#111";
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      ctx.fillStyle = "#888";
      ctx.font = `${Math.round(canvas.height / 12)}px sans-serif`;
      ctx.textAlign = "center";
      ctx.fillText(cam.connected === false ? "Camera not answering" : "Waiting for a picture", canvas.width / 2, canvas.height / 2);
    },
    forget: () => shots.clear(),
  };
}

function corners(ctx, x, y, w, h, len) {
  ctx.beginPath();
  for (const [cx, cy, dx, dy] of [[x, y, 1, 1], [x + w, y, -1, 1], [x, y + h, 1, -1], [x + w, y + h, -1, -1]]) {
    ctx.moveTo(cx + dx * len, cy);
    ctx.lineTo(cx, cy);
    ctx.lineTo(cx, cy + dy * len);
  }
  ctx.stroke();
}

/** Boxes round everyone the tracker sees; the followed one bold, with its name tag. */
export function drawPeople(ctx, W, H, seen, colours) {
  if (!seen || !seen.people) return;
  const k = W / 640;
  const placed = [];
  // The placeholder line centres its text; the tags start at their left.
  ctx.textAlign = "left";
  for (const p of seen.people) {
    const [x, y, w, h] = [p.box[0] * W, p.box[1] * H, p.box[2] * W, p.box[3] * H];
    const followed = p.id === seen.following;
    ctx.lineWidth = (followed ? 4 : 2) * k;
    ctx.strokeStyle = followed ? colours.follow : "rgba(255,255,255,0.75)";
    if (followed) {
      ctx.fillStyle = colours.followFill;
      ctx.fillRect(x, y, w, h);
      corners(ctx, x, y, w, h, Math.min(w, h) * 0.28);
    } else {
      ctx.setLineDash([6 * k, 5 * k]);
      ctx.strokeRect(x, y, w, h);
      ctx.setLineDash([]);
    }
    const label = followed ? "Following" : "Follow";
    ctx.font = `700 ${Math.round(17 * k)}px ui-sans-serif, system-ui, sans-serif`;
    const tw = ctx.measureText(label).width + 12 * k;
    const [tx, ty] = tagSpot(placed, Math.min(Math.max(0, x), W - tw), Math.max(0, y - 26 * k), tw, 24 * k, H);
    ctx.fillStyle = followed ? colours.follow : "rgba(0,0,0,0.55)";
    ctx.fillRect(tx, ty, tw, 24 * k);
    ctx.fillStyle = followed ? colours.onFollow : "#fff";
    ctx.fillText(label, tx + 6 * k, ty + 18 * k);
  }
}

/** Where a name tag goes: its wanted spot, or lower until it covers no tag
 *  already drawn, so two people standing close keep readable tags. */
export function tagSpot(placed, x, y, w, h, H) {
  const hits = (ty) => placed.some((r) => x < r[0] + r[2] && x + w > r[0] && ty < r[1] + r[3] && ty + h > r[1]);
  let ty = y;
  while (hits(ty) && ty + h < H) ty += h + 2;
  placed.push([x, ty, w, h]);
  return [x, ty];
}

/** The person under a point of the picture, given as fractions of it. */
export function personAt(seen, x, y) {
  if (!seen || !seen.people) return null;
  const pad = 0.03;
  const hit = seen.people.filter((p) => x >= p.box[0] - pad && x <= p.box[0] + p.box[2] + pad && y >= p.box[1] - pad && y <= p.box[1] + p.box[3] + pad);
  hit.sort((a, b) => Math.abs(x - a.box[0] - a.box[2] / 2) - Math.abs(x - b.box[0] - b.box[2] / 2));
  return hit[0] ? hit[0].id : null;
}
