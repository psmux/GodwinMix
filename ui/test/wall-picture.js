// The wall stub's pictures: bars with the show's name, or black for a
// show gone black. Drawn once per show.

/** A picture per show, drawn once: bars, or black for a show gone black. */
export function picture(show, i) {
  const c = Object.assign(document.createElement("canvas"), { width: 160, height: 90 });
  const g = c.getContext("2d");
  const hue = (i * 47) % 360;
  const grad = g.createLinearGradient(0, 0, 160, 90);
  grad.addColorStop(0, `hsl(${hue} 45% 32%)`);
  grad.addColorStop(1, `hsl(${(hue + 40) % 360} 50% 18%)`);
  const dark = show.health.alarms.some((a) => /black|no-input/.test(a.kind)) || show.state === "stopped";
  g.fillStyle = dark ? "#000" : grad;
  g.fillRect(0, 0, 160, 90);
  if (dark) return c.toDataURL("image/png");
  Object.assign(g, { fillStyle: "rgba(255,255,255,0.85)", font: "bold 15px system-ui" }).fillText(show.name, 10, 52);
  g.fillStyle = "rgba(255,255,255,0.35)";
  g.fillRect(10, 62, 60, 3);
  return c.toDataURL("image/png");
}
