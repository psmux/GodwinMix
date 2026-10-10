// The publisher's controls, each to what it does, and the page and network
// events it listens to. Answers the function that takes those off again.
//
// The network events are what make a publish come back by itself the moment
// it can: `online` when a lost network returns, the Network Information
// `change` when a phone moves from Wi-Fi to cellular (Chrome on Android has
// it; elsewhere `online` and the connection's own state do the work), and
// the page being shown again after it was in the background, where a
// browser slows its timers to one a minute.

import { refreshDevices } from "./tracks.js";
import { flipCamera } from "./flip.js";

export function wire(pub) {
  const r = pub.r;
  r.go.onclick = () => (pub.session && pub.session.active ? pub.stopPublishing() : pub.startPublishing());
  r.camera.onchange = () => pub.switchTo("video", r.camera.value);
  r.mic.onchange = () => pub.switchTo("audio", r.mic.value);
  r.processing.onchange = () => pub.switchTo("audio", r.mic.value);
  r.cameraMute.onclick = () => pub.toggle("video");
  r.micMute.onclick = () => pub.toggle("audio");
  r.flip.onclick = async () => {
    r.flip.disabled = true;
    await flipCamera(pub).catch(() => {});
    r.flip.disabled = false;
  };
  const nudge = (moved) => pub.session && pub.session.nudge(moved);
  const onDevices = () => refreshDevices(pub);
  const onHidden = () => {
    pub.applyVisibility();
    if (!document.hidden) nudge(false);
  };
  const onOnline = () => nudge(true);
  const onMoved = () => nudge(true);
  const net = navigator.connection;
  navigator.mediaDevices.addEventListener("devicechange", onDevices);
  document.addEventListener("visibilitychange", onHidden);
  window.addEventListener("online", onOnline);
  if (net && net.addEventListener) net.addEventListener("change", onMoved);
  return () => {
    navigator.mediaDevices.removeEventListener("devicechange", onDevices);
    document.removeEventListener("visibilitychange", onHidden);
    window.removeEventListener("online", onOnline);
    if (net && net.removeEventListener) net.removeEventListener("change", onMoved);
  };
}
