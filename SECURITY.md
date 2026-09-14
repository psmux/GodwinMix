# Security

## Reporting a problem

Mail **maintainers@example.invalid** with what you found and how to reproduce it. Do
not open a public issue.

If you would rather not use mail, open a
[private security advisory](https://github.com/psmux/GodwinMix/security/advisories/new)
on GitHub, which reaches the same person and keeps the report private until
there is a fix.

What to include, in whatever detail you have:

* What an attacker can do, in one sentence.
* The steps to reproduce it, and the config file if it matters.
* The version (`godwinmix --version`) and the platform.
* Whether it needs the control token, a local account, or nothing at all.

## What happens next

| When | What |
|---|---|
| Within 48 hours | An acknowledgement from a person, saying whether it is understood or whether more is needed |
| Within 7 days | An assessment: whether it is a vulnerability, how bad, and a rough date |
| On release | A fix, a GitHub advisory, and credit to you by whatever name you choose, unless you would rather not be named |

If 48 hours pass with nothing, assume the mail was missed and say so again. That
is not rudeness, it is the correct response.

Please give a reasonable amount of time before disclosing publicly. There is no
bounty programme; there is no money in this project to fund one.

## What counts

**In scope.** Anything that lets somebody who should not have control of a
mixer get it, or that lets somebody who can reach one port reach further than
that port should allow.

* Bypassing the control token on any `/api/*` route or the WebSocket.
* Reaching the filesystem or the network through a source URI, a media path or
  an upload beyond what the config allows.
* Command execution that does not require `allow_exec_sources = true`.
* Anything in the browser sidecar that lets a rendered page reach the host.
* Memory safety problems reachable from a network source or a control request.
* Secrets (the token, stream keys) appearing in logs, in the support bundle, or
  in a response body.

**Known and deliberate, so not a vulnerability.**

* **`allow_exec_sources = true` is arbitrary code execution** for anyone who can
  reach the control port. That is what it is for and it is off by default. The
  documentation says so in every place it appears.
* **A mixer with no token is open to whoever can reach the port.** The mixer
  warns, the documentation says to set one, and it stays possible because a
  mixer on a trusted LAN behind a firewall is a legitimate deployment.
* **The mixer does not terminate TLS.** It speaks plain HTTP and expects a
  reverse proxy in front of it. That is a design decision, documented in
  [deploy/README.md](deploy/README.md).
* **`web+` pages run a real browser.** A page you point the mixer at runs its
  own JavaScript, as a browser does. Do not point it at a page you do not
  trust. A sandbox escape from that browser is in scope; the page executing at
  all is not.
* **Stream keys in a config file** are as protected as the file is. Use
  `GODWINMIX_TOKEN` and a `0600` environment file for the control token; output
  URIs currently live in the config. Moving secrets to a separate store is
  planned.

## Supported versions

Only the latest release. This project is young enough that backporting to an
older one would be a promise it cannot keep. When that changes, this section
will say so.

## Hardening a deployment

[deploy/README.md](deploy/README.md) is the operational page: the token, the
reverse proxy, the firewall, and why every published port in the compose file
is bound to `127.0.0.1`.

Three things worth repeating here:

1. Set the token before the port is reachable from anywhere but your own
   machine.
2. Bind to loopback and put TLS in front. A published Docker port bypasses ufw
   on most systems, because Docker writes its own rules ahead of yours.
3. Leave `allow_exec_sources` off unless the control port is on a network you
   trust completely.
