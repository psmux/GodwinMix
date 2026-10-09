# Serve the control port over HTTPS

A browser only lets a page use the camera and microphone over `https://`, or
at `http://localhost`. Open the mixer from another laptop at
`http://192.168.1.20:8080` and the browser quietly hides both. The mixer
answers HTTPS itself, on the same port as HTTP, so that laptop can open
`https://192.168.1.20:8080` instead.

Nothing changes for `http://`. Scripts, bookmarks and the desktop app keep
working on the address they already use.

## It is on already

HTTPS is on by default. On the first start the mixer makes a certificate for
this machine and prints where to open it, and where a phone gets what it
needs to trust it:

```text
GodwinMix is running. Open http://127.0.0.1:18650/ in a browser.
HTTPS is on the same port: https://192.168.77.173:18650/ (certificate fingerprint 45:BC:C5:5D:18:0A:0F:59:40:89:1F:B0:A5:70:17:CE:5C:6E:22:0B:77:6F:21:85:82:F7:81:BE:08:32:FE:71).
To trust it on a phone, install its authority from https://192.168.77.173:18650/ca.crt (fingerprint 2E:62:B7:DC:1D:A5:1C:D1:34:7F:7E:D2:6C:C0:D5:01:BF:15:C7:5F:F3:AE:6C:F0:F2:FB:94:5D:78:CF:6D:1D).
```

That run was bound to port 18650; yours says 8080 unless you changed
`[control] bind`.

The certificate covers `localhost`, `127.0.0.1`, `::1`, the machine's name
and its LAN address. It is signed by a small certificate authority the mixer
makes once for this machine, `GodwinMix local authority on <machine name>`.
No browser knows that authority until someone tells it to, so the first time
a browser opens the https address it shows a warning. Check the fingerprint
against the one the mixer printed, then accept it. The browser remembers that
for this address.

Or trust the authority instead, once, on each device: the browser stops
warning altogether, the page can be installed as an app, and it keeps working
when the mixer makes a new certificate. For a phone,
[install the control page as an app](install-as-an-app.md#trust-this-mixer-on-a-phone)
has the steps for iOS and Android. On a computer, import
`<config name>.control.crt` into the system's trust store as a root
authority.

The certificate is kept and used again on every start, so the warning comes
once. A new one is made when the machine's LAN address changes (the old one
would not cover the new address) and a month before it runs out, after 800
days. Either way the certificate's fingerprint changes and a browser that
only accepted the warning asks again; a device that trusts the authority does
not notice. The authority lasts ten years.

A mixer that kept a self signed certificate from before it had an authority
makes the authority and a new certificate on its first start, and prints the
new fingerprints.

## Where it is kept

* Both private keys, the certificate's and the authority's, are sealed in the
  secret store under `GODWINMIX_HOME` (`~/.godwinmix/secrets`), the same store
  the RTMPS certificate and plugin passwords use. Neither is ever written to a
  plain file. One store means one authority for every mixer config on the
  machine.
* The authority's certificate, which is public, is written beside the config
  file as `<config name>.control.crt`: `godwinmix.control.crt` for
  `godwinmix.toml`. The mixer also serves it, with no token, at `/ca.crt`
  (as `application/x-x509-ca-cert`, which iOS opens as a profile) and at
  `/ca.pem` (the same file as a plain download, for Android). Import it into
  another machine's trust store and its browsers stop warning at all.

## Check it from a terminal

```sh
curl -s --cacert godwinmix.control.crt https://localhost:18650/api/v1/core/info | jq .tls
```

```json
{
  "authority": "2E:62:B7:DC:1D:A5:1C:D1:34:7F:7E:D2:6C:C0:D5:01:BF:15:C7:5F:F3:AE:6C:F0:F2:FB:94:5D:78:CF:6D:1D",
  "fingerprint": "45:BC:C5:5D:18:0A:0F:59:40:89:1F:B0:A5:70:17:CE:5C:6E:22:0B:77:6F:21:85:82:F7:81:BE:08:32:FE:71",
  "names": [
    "localhost",
    "127.0.0.1",
    "::1",
    "uniqueduo.local",
    "uniqueduo",
    "192.168.77.173"
  ],
  "source": "self_signed",
  "urls": [
    "https://192.168.77.173:18650/",
    "https://uniqueduo.local:18650/",
    "https://uniqueduo:18650/",
    "https://localhost:18650/"
  ]
}
```

`--cacert` takes the authority, and the port's certificate is checked against
it. On Windows, the curl that ships with Git and with Windows uses Schannel,
which also asks whether the certificate has been revoked, finds nowhere to
ask, and stops with `schannel: the revocation status is unknown`; add
`--ssl-no-revoke` there.

`core.info` carries `tls` whenever HTTPS is on, so a page or a script can show
the address and both fingerprints without reading the terminal. `authority`
is there only for a certificate the mixer made. With a token set, add
`-H "Authorization: Bearer $TOKEN"`.

The WebSocket upgrades over TLS like it does over HTTP:

```sh
curl -s -i -N --max-time 2 -H "Connection: Upgrade" -H "Upgrade: websocket" \
  -H "Sec-WebSocket-Version: 13" -H "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==" \
  --cacert godwinmix.control.crt "https://localhost:18650/rpc" | head -3
```

```text
HTTP/1.1 101 Switching Protocols
connection: upgrade
upgrade: websocket
```

`openssl` shows the authority's fingerprint the mixer printed, and that it is
an authority:

```sh
openssl x509 -in godwinmix.control.crt -noout -subject -dates -fingerprint -sha256 -ext basicConstraints,keyUsage
```

```text
subject=CN=GodwinMix local authority on uniqueduo, O=GodwinMix
notBefore=Oct  8 00:00:00 2026 GMT
notAfter=Oct  6 00:00:00 2036 GMT
sha256 Fingerprint=2E:62:B7:DC:1D:A5:1C:D1:34:7F:7E:D2:6C:C0:D5:01:BF:15:C7:5F:F3:AE:6C:F0:F2:FB:94:5D:78:CF:6D:1D
X509v3 Key Usage: critical
    Digital Signature, Certificate Sign, CRL Sign
X509v3 Basic Constraints: critical
    CA:TRUE, pathlen:0
```

The certificate the port serves is the other half. Save it with
`openssl s_client` and check it against the authority:

```sh
echo | openssl s_client -connect 192.168.77.173:18650 -servername 192.168.77.173 2>/dev/null | openssl x509 > port.pem
openssl x509 -in port.pem -noout -subject -issuer -ext basicConstraints,subjectAltName
openssl verify -CAfile godwinmix.control.crt port.pem
```

```text
subject=CN=GodwinMix on uniqueduo
issuer=CN=GodwinMix local authority on uniqueduo, O=GodwinMix
X509v3 Subject Alternative Name:
    DNS:localhost, IP Address:127.0.0.1, IP Address:0:0:0:0:0:0:0:1, DNS:uniqueduo.local, DNS:uniqueduo, IP Address:192.168.77.173
X509v3 Basic Constraints: critical
    CA:FALSE
port.pem: OK
```

## Use your own certificate

If the mixer has a real name and a certificate for it, point the config at
the two PEM files. Relative paths are read from the config file's folder.

```toml
[control.tls]
cert = "/etc/ssl/mixer.example.com/fullchain.pem"
key = "/etc/ssl/mixer.example.com/privkey.pem"
```

Or from a terminal, as with any other setting
([change a setting](change-a-setting.md)):

```sh
curl -s -X POST -H "authorization: Bearer $GODWINMIX_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"values": {"control.tls.cert": "/etc/ssl/mixer.example.com/fullchain.pem",
                  "control.tls.key": "/etc/ssl/mixer.example.com/privkey.pem"}}' \
  http://127.0.0.1:8080/api/v1/config/set
```

Both take effect when the mixer restarts. `core.info` then says
`"source": "files"`. Setting both back to an empty string, or resetting them,
returns to the certificate the mixer makes.

When a file cannot be read, or the key does not belong to the certificate,
the mixer still starts and serves plain HTTP, so you can reach it to fix the
setting. It says what went wrong on the terminal and as an alert in the page:

```text
HTTPS is off on the control port, plain HTTP still works: [control.tls] cert is /etc/ssl/mixer.example.com/fullchain.pem, which cannot be read (No such file or directory (os error 2)). Fix the path, or remove cert and key to use a certificate this mixer makes.
```

## Turn it off

```toml
[control.tls]
enabled = false
```

The port then answers plain HTTP only, as it did before. Do this when a
[reverse proxy](reverse-proxy.md) already does TLS in front of the mixer and
you want nothing on the port the proxy does not use.

## What is not here yet

Let's Encrypt (ACME) is not built yet. For a public name with a certificate
that renews itself, put [a reverse proxy](reverse-proxy.md) such as Caddy in
front for now.
