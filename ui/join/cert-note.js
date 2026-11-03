// The one line a guest needs about the browser's certificate warning. The
// mixer serves https with a certificate it signed itself, so the first visit
// to its address stops at "Your connection is not private". The line shows
// only for an address no public certificate authority signs for: an IP
// address, a `.local` name or a bare machine name. A tunnel or a real domain
// name has a certificate of its own and no warning to explain.

/** On the guest's own page. */
export const CERT_GUEST =
  "The browser may warn that the connection is not private because the mixer signs its own certificate. Choose Advanced, then continue.";

/** In the invite dialog, for whoever sends the link. */
export const CERT_INVITE =
  "The guest's browser warns that the connection is not private because the mixer signs its own certificate. Tell them to choose Advanced, then continue.";

/** Does an address at `href` get the mixer's own certificate? */
export function ownCertificate(href) {
  let url;
  try {
    url = new URL(href);
  } catch {
    return false;
  }
  if (url.protocol !== "https:") return false;
  const host = url.hostname.replace(/^\[|\]$/g, "");
  const ip = /^\d{1,3}(\.\d{1,3}){3}$/.test(host) || host.includes(":");
  return ip || host.endsWith(".local") || !host.includes(".");
}
