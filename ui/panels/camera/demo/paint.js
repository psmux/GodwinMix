// Draws what a demo camera sees: a small stage with a screen, a lectern, two
// plants and the people walking on it, through the camera's pan, tilt and
// zoom. Stage units are fractions of a 16:9 stage, so a head is an ellipse
// 9/16 as wide in x as it is tall in y and comes out round on screen.

import { FLOOR, seenX, viewRect } from "./world.js";

const R = 9 / 16;

function room(ctx, cam) {
  const wall = ctx.createLinearGradient(0, 0, 0, FLOOR);
  wall.addColorStop(0, "#2b2622");
  wall.addColorStop(1, "#5a4a3c");
  ctx.fillStyle = wall;
  ctx.fillRect(-0.2, -0.2, 1.4, FLOOR + 0.2);
  ctx.fillStyle = "rgba(255,255,255,0.035)";
  for (let x = 0; x < 1; x += 0.08) ctx.fillRect(x, 0, 0.004, FLOOR);
  for (const x of [0.12, 0.5, 0.88]) {
    const g = ctx.createRadialGradient(x, 0, 0, x, 0, 0.5);
    g.addColorStop(0, "rgba(255,228,180,0.28)");
    g.addColorStop(1, "rgba(255,228,180,0)");
    ctx.fillStyle = g;
    ctx.fillRect(x - 0.5, 0, 1, 0.6);
  }
  screen(ctx, cam);
  ctx.fillStyle = "#3b2c22";
  ctx.fillRect(-0.2, FLOOR, 1.4, 0.4);
  ctx.fillStyle = "#6b4d36";
  ctx.fillRect(-0.2, FLOOR, 1.4, 0.012);
  for (const x of [0.08, 0.92]) plant(ctx, seenX(cam, x));
}

function screen(ctx, cam) {
  const x = seenX(cam, 0.5) - 0.16;
  ctx.fillStyle = "#0d1a2e";
  ctx.fillRect(x - 0.006, 0.1 - 0.01, 0.332, 0.32);
  const g = ctx.createLinearGradient(x, 0.1, x + 0.32, 0.4);
  g.addColorStop(0, "#1f4f8a");
  g.addColorStop(1, "#5a3f8f");
  ctx.fillStyle = g;
  ctx.fillRect(x, 0.1, 0.32, 0.3);
  ctx.fillStyle = "rgba(255,255,255,0.85)";
  ctx.fillRect(x + 0.06, 0.18, 0.2, 0.03);
  ctx.fillStyle = "rgba(255,255,255,0.45)";
  for (let i = 0; i < 3; i++) ctx.fillRect(x + 0.08, 0.25 + i * 0.04, 0.16 - i * 0.02, 0.014);
}

function plant(ctx, x) {
  ctx.fillStyle = "#4a3426";
  ctx.fillRect(x - 0.02, FLOOR - 0.08, 0.04, 0.08);
  ctx.fillStyle = "#2f6b3a";
  for (let i = 0; i < 6; i++) {
    ctx.beginPath();
    ctx.ellipse(x + (i - 2.5) * 0.008, FLOOR - 0.13 - (i % 3) * 0.03, 0.02 * R * 1.6, 0.05, (i - 2.5) * 0.3, 0, Math.PI * 2);
    ctx.fill();
  }
}

function lectern(ctx, cam) {
  const x = seenX(cam, 0.5);
  ctx.fillStyle = "#5b3b25";
  ctx.beginPath();
  ctx.moveTo(x - 0.045, FLOOR - 0.24);
  ctx.lineTo(x + 0.045, FLOOR - 0.24);
  ctx.lineTo(x + 0.035, FLOOR);
  ctx.lineTo(x - 0.035, FLOOR);
  ctx.fill();
  ctx.fillStyle = "#7a5236";
  ctx.fillRect(x - 0.05, FLOOR - 0.255, 0.1, 0.02);
}

function person(ctx, cam, p, t) {
  const x = seenX(cam, p.x);
  const h = p.h;
  const top = FLOOR - h;
  const walking = p.wait <= 0;
  const swing = walking ? Math.sin(t * 7 + h * 40) * 0.012 : 0;
  ctx.fillStyle = "#23252b";
  ctx.fillRect(x - 0.022 + swing, top + h * 0.58, 0.018, h * 0.42);
  ctx.fillRect(x + 0.004 - swing, top + h * 0.58, 0.018, h * 0.42);
  ctx.fillStyle = p.coat;
  ctx.beginPath();
  ctx.roundRect(x - 0.034, top + h * 0.2, 0.068, h * 0.42, 0.012);
  ctx.fill();
  ctx.fillRect(x - 0.046 - swing, top + h * 0.22, 0.014, h * 0.32);
  ctx.fillRect(x + 0.032 + swing, top + h * 0.22, 0.014, h * 0.32);
  ctx.fillStyle = p.skin;
  ctx.fillRect(x - 0.008, top + h * 0.15, 0.016, h * 0.07);
  ctx.beginPath();
  ctx.ellipse(x, top + h * 0.09, h * 0.09 * R, h * 0.09, 0, 0, Math.PI * 2);
  ctx.fill();
  ctx.fillStyle = p.hair;
  ctx.beginPath();
  ctx.ellipse(x, top + h * 0.06, h * 0.092 * R, h * 0.065, 0, Math.PI, Math.PI * 2);
  ctx.fill();
}

/** Paint camera `cam` of world `w` into a 2D context of W by H pixels. */
export function paintCamera(ctx, W, H, w, cam) {
  const [vx, vy, vw, vh] = viewRect(cam.view);
  const sx = W / vw;
  const sy = H / vh;
  ctx.setTransform(sx, 0, 0, sy, -vx * sx, -vy * sy);
  room(ctx, cam);
  const order = [...w.people].sort((a, b) => Math.abs(seenX(cam, a.x) - 0.5) - Math.abs(seenX(cam, b.x) - 0.5));
  lectern(ctx, cam);
  for (const p of order) person(ctx, cam, p, w.t);
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  const b = cam.exposure.brightness || 0;
  if (b) {
    ctx.fillStyle = b > 0 ? `rgba(255,255,255,${b * 0.35})` : `rgba(0,0,0,${-b * 0.5})`;
    ctx.fillRect(0, 0, W, H);
  }
  if (cam.look.tint) {
    ctx.fillStyle = cam.look.tint;
    ctx.fillRect(0, 0, W, H);
  }
}
