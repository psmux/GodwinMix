# Running GodwinMix on a server

The mixer is headless. It has no window, no desktop dependency and no
interactive prompt; the UI is a web page it serves on one port, and everything
that page can do is an HTTP call anything else can make. That is the shape a
server wants, so there is not much to this.

Three ways to install it, in the order of how much you want to think about:

| | |
|---|---|
| [Docker](docker/) | one image, the GStreamer plugins are inside it, nothing on the host |
| [systemd](systemd/godwinmix.service) | one binary, the distribution's GStreamer packages, a unit file |
| by hand | `cargo build --release` and run it under whatever you already use |

The rest of this page is the part that is the same either way: the token, the
proxy, the firewall, and what to watch.

## What has to be reachable, and by whom

| Port | Who needs it | Exposed to the internet? |
|---|---|---|
| 8080, the control port | operators, agents, your backend | only through a reverse proxy with TLS |
| 1935 (or wherever the sources publish) | cameras, encoders, phones | only if the sources are off site |
| the outputs' addresses | outbound only | nothing to open |

The outputs are connections the mixer makes, not connections it accepts. A
mixer that only publishes to YouTube needs no inbound port at all except the
control port, and that one only for you.

## The token

Set one before the control port answers anything but your own machine. Without
a token, whoever can reach port 8080 can take a source, add an output pointing
at their own server, and shut the mixer down.

```sh
# On the server, once:
head -c 32 /dev/urandom | base64 > /etc/godwinmix/token
```

Give it to the mixer as `GODWINMIX_TOKEN` in the environment rather than as
`[control] token` in the config file. The environment variable wins over the
file, secrets in config files get committed, and a systemd `EnvironmentFile`
or a Docker secret keeps it out of `ps` and out of the image.

Every `/api/*` call and the WebSocket then need `Authorization: Bearer <token>`.
A GET, which is what a WebSocket upgrade is, may carry `?token=<token>`
instead, because a browser cannot put a header on a socket. The UI asks for the
token once and keeps it in the browser's local storage. `gmx ctl` reads
`GODWINMIX_TOKEN` from its own environment, so scripts on the server need no
flag.

```sh
GODWINMIX_TOKEN="$(cat /etc/godwinmix/token)" gmx ctl status
```

## Behind a reverse proxy with TLS

The mixer speaks plain HTTP and does not terminate TLS. Bind it to loopback and
put a proxy in front. Two things the proxy must get right: the WebSocket
upgrade, and not touching the HTML.

Caddy, which gets a certificate on its own:

```caddyfile
mixer.example.com {
	# The UI is one HTML page with inline scripts and a WebSocket. A proxy or
	# CDN that rewrites scripts or caches the HTML breaks it.
	header Cache-Control "no-store, no-transform"

	reverse_proxy 127.0.0.1:8080 {
		# Multiview frames arrive as binary WebSocket messages every 125 ms.
		# A proxy read timeout shorter than that closes the socket mid stream.
		transport http {
			read_timeout 300s
		}
	}
}
```

nginx, if that is what is already on the box:

```nginx
server {
    listen 443 ssl http2;
    server_name mixer.example.com;

    # certificates from certbot or wherever you get them

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;

        # Without these two the WebSocket upgrade becomes a plain GET and the
        # UI reconnects forever with no multiview and no events.
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";

        proxy_set_header Host $host;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;

        # The multiview socket is quiet between frames when nobody is
        # watching. A 60 second default closes it.
        proxy_read_timeout 300s;
        proxy_buffering off;
        add_header Cache-Control "no-store, no-transform" always;
    }
}
```

Behind Cloudflare, turn Rocket Loader off for this hostname and do not cache
the HTML. Rocket Loader rewrites inline scripts and the UI stops loading.

## Firewall

```sh
# Debian or Ubuntu with ufw, a server whose sources are on the same LAN.
ufw default deny incoming
ufw allow 22/tcp
ufw allow 443/tcp                      # the proxy, not the mixer
ufw allow from 192.168.1.0/24 to any port 1935 proto tcp
ufw enable
```

The control port itself is never in that list, because it is bound to
`127.0.0.1` and reached through the proxy. If you have to bind it to a real
interface (another machine on the LAN runs the UI, say), allow it from that
network and nowhere else, and set the token anyway.

With Docker, note that a published port bypasses ufw on most systems, because
Docker writes its own rules into the `DOCKER-USER` chain ahead of yours. That
is why every port in `docker-compose.yml` is published as `127.0.0.1:PORT:PORT`
rather than `PORT:PORT`. Check with `ss -ltnp` that nothing is listening on
`0.0.0.0` that you did not mean.

## Starting with the sources it should have

A mixer restarted at 6am should come back with the same cameras. Two ways:

* Put them in the config as `[[sources]]` and `[[outputs]]`. That is the right
  answer for a box that always mixes the same thing.
* Add them over the API. The mixer writes them to `godwinmix.runtime.toml`
  beside its config and reads that file back on the next start. Once that file
  exists, it is the list; the config's own sources are not merged in, so a
  source you deleted in the UI does not come back. Delete the runtime file to
  go back to the config.

## Watching it

```sh
journalctl -u godwinmix -f                 # systemd
docker logs -f godwinmix                   # Docker

gmx ctl status                             # what is on programme, sources, outputs
curl -s localhost:8080/api/status | jq .    # the same, raw
```

What a monitoring check should look at, in order of how much it tells you:

1. `GET /api/status` answers at all. If it does not, the process is gone.
2. Every output's state is `live`. An output that is `connecting` for more
   than a minute is a destination problem, not a mixer problem.
3. The programme source is what you expect. A mixer showing the slate at
   10am on a Sunday is a dead camera nobody noticed.

## Upgrading

Replace the binary or pull the new image and restart. The programme stops for
as long as the restart takes, so do it between broadcasts. Sources and outputs
come back from the runtime file. Config keys are not removed without a release
that says so.

## When it goes wrong on a server you cannot see

```sh
gmx ctl status                  # the state the mixer thinks it is in
godwinmix --probe               # the codecs it picked on this machine
journalctl -u godwinmix -n 200  # what it said when it broke
GST_DEBUG=3 godwinmix --config ...   # GStreamer's own account, in the log
```

`gmx doctor` (planned, Phase 0 of the roadmap) will name every missing
GStreamer element in one command, and `gmx support-bundle` (planned, Phase 2)
will collect the redacted config, every pipeline's graph and the session log
into one file to attach to an issue. Until they land, `--probe` and the log are
what there is.
