# Put it behind a reverse proxy with TLS

The mixer speaks plain HTTP on one port and does not terminate TLS. Bind it to
loopback, put a proxy in front, and the proxy does the certificate.

Two things a proxy has to get right here, and both of them are things a default
configuration gets wrong:

* **The WebSocket upgrade.** `GET /ws` carries the event stream and the
  multiview mosaic frames. A proxy that drops the `Upgrade` and `Connection`
  headers turns it into a plain GET, and the UI then reconnects forever with no
  multiview and no events, which looks like the mixer is broken.
* **Not touching the HTML.** The UI is one page with inline scripts. Anything
  that rewrites scripts (Cloudflare's Rocket Loader is the usual culprit) or
  caches the HTML breaks it.

## Caddy

Gets its own certificate and renews it. This is the whole file.

```caddyfile
mixer.example.com {
	header Cache-Control "no-store, no-transform"

	reverse_proxy 127.0.0.1:8080 {
		# Multiview frames arrive every 125 ms at 8 fps, and nothing at all
		# when no operator is watching. A read timeout shorter than a quiet
		# period closes the socket mid broadcast.
		transport http {
			read_timeout 300s
		}
	}
}
```

## nginx

```nginx
server {
    listen 443 ssl http2;
    server_name mixer.example.com;

    ssl_certificate     /etc/letsencrypt/live/mixer.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/mixer.example.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;

        # Without these two the WebSocket never upgrades.
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";

        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        proxy_read_timeout 300s;
        proxy_buffering off;
        add_header Cache-Control "no-store, no-transform" always;
    }
}
```

`proxy_buffering off` matters more than it looks. The snapshot and mosaic
routes stream, and a buffering proxy adds latency to the one thing an operator
is looking at while deciding whether to cut.

## Behind Cloudflare

* Turn Rocket Loader off for this hostname.
* Do not cache the HTML. A cached page with a stale inline script is a UI that
  half works and nobody can explain.
* WebSockets are on by default on every plan, but check.
* The mixer's own outbound connections (RTMP to your CDN) do not go through
  Cloudflare and are not affected by any of this.

## The token still matters

TLS stops somebody reading the traffic. It does not stop them making requests.
Set `GODWINMIX_TOKEN` as well. A mixer on a public hostname with no token is a
mixer anyone can take sources on.

The UI asks for the token once and keeps it in the browser's local storage. A
GET, and a WebSocket upgrade is a GET, may carry `?token=<token>` in the query
string instead of the header, because a browser cannot put a header on a
socket. That is how an `<img src="/api/snapshot/program.jpg?token=...">` works.

## Checking it

```sh
curl -sI https://mixer.example.com/ | head -5
curl -s -H "Authorization: Bearer $TOKEN" https://mixer.example.com/api/status | jq .program

# The upgrade, which is the one that catches people out.
curl -sI -H "Connection: Upgrade" -H "Upgrade: websocket" \
  -H "Sec-WebSocket-Version: 13" -H "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==" \
  "https://mixer.example.com/ws?token=$TOKEN" | head -3
```

The last one should answer `101 Switching Protocols`. A `200` means the proxy
ate the upgrade.
