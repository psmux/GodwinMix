// The publisher's controls, each to what it does, and the two page events it
// listens to. Answers the function that takes the page events off again.

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
  const onDevices = () => refreshDevices(pub);
  const onHidden = () => pub.applyVisibility();
  navigator.mediaDevices.addEventListener("devicechange", onDevices);
  document.addEventListener("visibilitychange", onHidden);
  return () => {
    navigator.mediaDevices.removeEventListener("devicechange", onDevices);
    document.removeEventListener("visibilitychange", onHidden);
  };
}
