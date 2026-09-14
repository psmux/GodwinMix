// Preview video for a surface that is not the reference UI.
//
// Three ways in, cheapest first:
//
//   snapshot   one JPEG on demand, for an agent or a still
//   MJPEG      a picture a second to thirty, no audio, ten lines of code
//   WHEP       WebRTC, audio included, under 500 ms, a real <video> element
//
// The URL builders live in urls.ts and work anywhere. What is here needs a
// browser: an <img>, a <video> and the platform's RTCPeerConnection.

/** Point an <img> at an MJPEG stream. Returns a function that stops it. */
export function attachMjpeg(img: HTMLImageElement, url: string): () => void {
  img.src = url;
  return () => {
    // Clearing src is what closes the HTTP body, and with it the core's
    // encoder for this stream. Leaving it set keeps the mixer working for a
    // picture nobody is looking at.
    img.removeAttribute("src");
  };
}

export interface WhepHandle {
  /** The connection, if a surface wants its stats or its ICE state. */
  pc: RTCPeerConnection;
  /** Close the session: DELETE the resource, then close the connection. */
  stop(): Promise<void>;
}

/**
 * Play a WHEP stream in a <video> element.
 *
 * Nothing here is GodwinMix specific: this is the WHEP handshake (an SDP offer
 * posted as `application/sdp`, the answer in the body, the session's own URL in
 * the Location header) over the platform's WebRTC API. It is here so that a
 * surface does not have to write it again.
 *
 * ```ts
 * const handle = await attachWhep(video, client.whepUrl("program"));
 * // later
 * await handle.stop();
 * ```
 */
export async function attachWhep(
  video: HTMLVideoElement,
  url: string,
  opts: { token?: string | null; iceServers?: RTCIceServer[] } = {},
): Promise<WhepHandle> {
  const pc = new RTCPeerConnection({ iceServers: opts.iceServers || [] });
  pc.addTransceiver("video", { direction: "recvonly" });
  pc.addTransceiver("audio", { direction: "recvonly" });

  const stream = new MediaStream();
  pc.ontrack = (ev) => {
    stream.addTrack(ev.track);
    if (video.srcObject !== stream) video.srcObject = stream;
  };

  const offer = await pc.createOffer();
  await pc.setLocalDescription(offer);
  await gathered(pc);

  const headers: Record<string, string> = { "content-type": "application/sdp" };
  if (opts.token) headers["authorization"] = "Bearer " + opts.token;
  const res = await fetch(url, {
    method: "POST",
    headers,
    body: pc.localDescription?.sdp || offer.sdp || "",
  });
  if (!res.ok) {
    pc.close();
    throw new Error(`WHEP at ${url} answered ${res.status}: ${await res.text()}`);
  }
  const answer = await res.text();
  await pc.setRemoteDescription({ type: "answer", sdp: answer });
  const resource = res.headers.get("location");

  return {
    pc,
    async stop() {
      pc.close();
      video.srcObject = null;
      if (!resource) return;
      try {
        await fetch(new URL(resource, url).toString(), { method: "DELETE", headers });
      } catch {
        // The core drops the session when the peer connection dies anyway.
        // A failed DELETE is tidiness, not correctness.
      }
    },
  };
}

/**
 * Wait for ICE to finish, or for a second, whichever is first.
 *
 * Trickle ICE would be better but WHEP's PATCH leg is optional and the core
 * does not need it on a LAN. A second is long enough for host candidates,
 * which is all a mixer on the same network offers.
 */
function gathered(pc: RTCPeerConnection): Promise<void> {
  if (pc.iceGatheringState === "complete") return Promise.resolve();
  return new Promise((resolve) => {
    const done = () => {
      pc.removeEventListener("icegatheringstatechange", check);
      clearTimeout(timer);
      resolve();
    };
    const check = () => {
      if (pc.iceGatheringState === "complete") done();
    };
    const timer = setTimeout(done, 1000);
    pc.addEventListener("icegatheringstatechange", check);
  });
}
