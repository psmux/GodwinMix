// One WHIP publish: an RTCPeerConnection, one POST with the offer, and a
// DELETE on the session's Location when it is over.
//
// The mixer takes no trickle ICE, so the offer goes once gathering is done
// with every candidate in it. A channel takes H.264 only (the picture is
// never decoded there), so H.264 goes first in the codec list.

/** What the encoder is asked for: a person talking, at 720p. */
export const ENCODER = { width: 1280, height: 720, frameRate: 30, maxBitrate: 2_500_000 };

/** A refusal from the WHIP endpoint, with the sentence it sent. */
export class WhipError extends Error {
  constructor(status, text) {
    super(text || `the mixer answered ${status}`);
    this.status = status;
  }
}

/**
 * Publish `media` ({video, audio}, either track may be null) to `url`.
 * Answers `{pc, senders, location}`. The caller watches `pc` for failure.
 */
export async function publish({ url, key, media, fetcher = fetch }) {
  const pc = new RTCPeerConnection({ bundlePolicy: "max-bundle" });
  try {
    const senders = addTransceivers(pc, media);
    await pc.setLocalDescription(await pc.createOffer());
    await gathered(pc, 4000);
    const res = await fetcher(url, {
      method: "POST",
      headers: { "Content-Type": "application/sdp", Authorization: `Bearer ${key}` },
      body: pc.localDescription.sdp,
    });
    const text = await res.text();
    if (res.status !== 201) throw new WhipError(res.status, text.trim());
    const base = new URL(url, document.baseURI);
    const location = new URL(res.headers.get("Location") || base.href, base).href;
    await pc.setRemoteDescription({ type: "answer", sdp: text });
    await preferSteadySize(senders.video);
    return { pc, senders, location };
  } catch (e) {
    pc.close();
    throw e;
  }
}

/** End a session. Best effort: the mixer also notices a peer that went away. */
export function unpublish(location, fetcher = fetch) {
  if (!location) return Promise.resolve();
  return fetcher(location, { method: "DELETE", keepalive: true }).catch(() => {});
}

function addTransceivers(pc, media) {
  // The video section is there even with no camera, because a channel
  // refuses an offer with no H.264 in it.
  const video = pc.addTransceiver(media.video || "video", {
    direction: "sendonly",
    sendEncodings: [{ maxBitrate: ENCODER.maxBitrate, maxFramerate: ENCODER.frameRate, scaleResolutionDownBy: 1 }],
  });
  preferH264(video);
  const audio = pc.addTransceiver(media.audio || "audio", { direction: "sendonly" });
  return { video: video.sender, audio: audio.sender };
}

function preferH264(transceiver) {
  const caps = window.RTCRtpSender?.getCapabilities?.("video");
  if (!caps || !transceiver.setCodecPreferences) return;
  const ordered = orderCodecs(caps.codecs);
  if (!ordered.some((c) => /h264/i.test(c.mimeType))) {
    throw new WhipError(0, "This browser cannot send H.264, which the mixer's channels need. Chrome, Edge, Safari and Firefox all can.");
  }
  transceiver.setCodecPreferences(ordered);
}

/**
 * Codecs with H.264 first: packetization mode 1 and constrained baseline
 * ahead of the other H.264 profiles, then the rest in the browser's order.
 */
export function orderCodecs(codecs) {
  const score = (c) => {
    if (!/h264/i.test(c.mimeType)) return 0;
    const fmtp = c.sdpFmtpLine || "";
    return 10 + (fmtp.includes("packetization-mode=1") ? 2 : 0) + (fmtp.includes("42e01f") ? 1 : 0);
  };
  return codecs
    .map((c, i) => [c, i])
    .sort((a, b) => score(b[0]) - score(a[0]) || a[1] - b[1])
    .map(([c]) => c);
}

/**
 * Keep the picture size steady and give up frame rate instead when the link
 * or the computer cannot keep up. A browser left to itself starts small and
 * raises the size as it finds bandwidth, and every change of size is a new
 * H.264 configuration, which the hop from a channel to the mixer carries only
 * once: the mixer's decoder then refuses every frame after the first change,
 * and the source freezes a few seconds in. A camera feeding a mixer wants one
 * size anyway.
 */
async function preferSteadySize(sender) {
  try {
    const params = sender.getParameters();
    params.degradationPreference = "maintain-resolution";
    for (const e of params.encodings || []) e.scaleResolutionDownBy = 1;
    await sender.setParameters(params);
  } catch {
    // Not every browser lets this be set. Its own default is close enough.
  }
}

/** Resolve when ICE gathering is complete, or after `ms` with what there is. */
function gathered(pc, ms) {
  if (pc.iceGatheringState === "complete") return Promise.resolve();
  return new Promise((resolve) => {
    const finish = () => {
      pc.removeEventListener("icegatheringstatechange", check);
      clearTimeout(timer);
      resolve();
    };
    const check = () => {
      if (pc.iceGatheringState === "complete") finish();
    };
    const timer = setTimeout(finish, ms);
    pc.addEventListener("icegatheringstatechange", check);
  });
}
