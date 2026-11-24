// Signing in with a username and password, for a mixer that has accounts.
//
// The session lives in an HttpOnly cookie the page never sees. A same origin
// fetch and the page's WebSocket carry it by themselves, so after a sign in
// the client connects with no token at all and the mixer knows who it is.

/**
 * Whether this mixer has accounts and who this browser is signed in as.
 * Never throws: a mixer that does not answer, or an older one without the
 * route, reads as "no accounts", which is how it behaved before.
 *
 * @returns {Promise<{accounts: boolean, user?: string, role?: string, via: string}>}
 */
export async function authStatus(base, token) {
  try {
    const headers = token ? { Authorization: "Bearer " + token } : {};
    const res = await fetch(new URL("/api/v1/auth/status", base), { headers, credentials: "same-origin" });
    if (!res.ok) return { accounts: false, via: "none" };
    return await res.json();
  } catch {
    return { accounts: false, via: "none" };
  }
}

/**
 * Sign in. Resolves to `{ok: true, user, role}` or `{ok: false, message,
 * wait}` with the mixer's own words for why.
 */
export async function signIn(base, username, password) {
  let res;
  try {
    res = await fetch(new URL("/api/v1/auth/login", base), {
      method: "POST",
      credentials: "same-origin",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ username, password }),
    });
  } catch {
    return { ok: false, message: "The mixer did not answer. Check that it is running and that this is its address, then try again." };
  }
  let body = {};
  try {
    body = await res.json();
  } catch {
    /* an answer with no body still has a status */
  }
  if (res.ok) return { ok: true, user: body.user, role: body.role };
  const message = (body.error && body.error.message) || `The mixer refused the sign in (${res.status}).`;
  return { ok: false, message, wait: res.status === 429 };
}

/** End this browser's session and forget its cookie. */
export async function signOut(base) {
  try {
    await fetch(new URL("/api/v1/auth/logout", base), { method: "POST", credentials: "same-origin" });
  } catch {
    /* signed out locally either way: the page reloads to the sign in */
  }
}
