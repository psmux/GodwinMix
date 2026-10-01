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
this machine and prints where to open it:

```text
GodwinMix is running. Open http://127.0.0.1:18080/ in a browser.
HTTPS is on the same port: https://192.168.77.106:18080/ (certificate fingerprint B6:F2:2D:5F:D8:4D:91:B0:39:6B:4D:75:F8:0A:BF:2E:B2:70:D8:96:F5:65:7D:CB:8E:74:8D:D9:10:18:96:7A).
```

That run was bound to port 18080; yours says 8080 unless you changed
`[control] bind`.

The certificate covers `localhost`, `127.0.0.1`, `::1`, the machine's name
and its LAN address. It is self signed, so the first time a browser opens the
https address it shows a warning. Check the fingerprint against the one the
mixer printed, then accept it. The browser remembers that for this address.

The certificate is kept and used again on every start, so the warning comes
once. A new one is made when the machine's LAN address changes (the old one
would not cover the new address) and a month before it runs out, after 800
days. Either way the fingerprint changes and the browser asks again.

## Where it is kept

* The private key is sealed in the secret store under `GODWINMIX_HOME`
  (`~/.godwinmix/secrets`), the same store the RTMPS certificate and plugin
  passwords use. It is never written to a plain file.
* The certificate itself, which is public, is written beside the config file
  as `<config name>.control.crt`: `godwinmix.control.crt` for
  `godwinmix.toml`. Import that file into another machine's trust store and
  its browsers stop warning at all.

## Check it from a terminal

```sh
curl -s --cacert godwinmix.control.crt https://localhost:18080/api/v1/core/info | jq .tls
```

```json
{
  "fingerprint": "B6:F2:2D:5F:D8:4D:91:B0:39:6B:4D:75:F8:0A:BF:2E:B2:70:D8:96:F5:65:7D:CB:8E:74:8D:D9:10:18:96:7A",
  "names": [
    "localhost",
    "127.0.0.1",
    "::1",
    "godwins-macbook-pro.local",
    "192.168.77.106"
  ],
  "source": "self_signed",
  "urls": [
    "https://192.168.77.106:18080/",
    "https://godwins-macbook-pro.local:18080/",
    "https://localhost:18080/"
  ]
}
```

`core.info` carries `tls` whenever HTTPS is on, so a page or a script can show
the address and the fingerprint without reading the terminal. With a token set,
add `-H "Authorization: Bearer $TOKEN"`.

The WebSocket upgrades over TLS like it does over HTTP:

```sh
curl -s -i -N --max-time 2 -H "Connection: Upgrade" -H "Upgrade: websocket" \
  -H "Sec-WebSocket-Version: 13" -H "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==" \
  --cacert godwinmix.control.crt "https://localhost:18080/rpc" | head -3
```

```text
HTTP/1.1 101 Switching Protocols
connection: upgrade
upgrade: websocket
```

`openssl` shows the same fingerprint the mixer printed:

```sh
openssl x509 -in godwinmix.control.crt -noout -fingerprint -sha256 -subject -dates
```

```text
SHA256 Fingerprint=B6:F2:2D:5F:D8:4D:91:B0:39:6B:4D:75:F8:0A:BF:2E:B2:70:D8:96:F5:65:7D:CB:8E:74:8D:D9:10:18:96:7A
subject= /CN=GodwinMix on godwins-macbook-pro.local
notBefore=Sep 30 00:00:00 2026 GMT
notAfter=Dec  9 00:00:00 2028 GMT
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
