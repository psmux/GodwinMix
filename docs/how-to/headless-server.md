# Run it on a headless server

A mixer on a box with no screen, started at boot, driven from a browser or a
script somewhere else. This is the deployment the project is built for.

Two ways. Pick one.

## With Docker

Everything GStreamer needs is inside the image, so the host carries nothing but
Docker.

```sh
mkdir -p /srv/godwinmix/config /srv/godwinmix/media
head -c 32 /dev/urandom | base64 > /srv/godwinmix/token

docker run -d --name godwinmix \
  --restart unless-stopped \
  -e GODWINMIX_SUPERVISED=1 \
  --shm-size 1g \
  -p 127.0.0.1:8080:8080 \
  -e GODWINMIX_TOKEN="$(cat /srv/godwinmix/token)" \
  -v /srv/godwinmix/config:/etc/godwinmix \
  -v /srv/godwinmix/media:/var/lib/godwinmix/media \
  ghcr.io/psmux/godwinmix:latest
```

The config directory can be empty: the entrypoint writes the shipped default
into it on first start, and the mixer writes `godwinmix.runtime.toml` beside it
as sources are added.

The build arguments (`WITH_X264`, `WITH_WPE`) and the GPU flags are in
[deploy/docker/README.md](../../deploy/docker/README.md).

## With systemd

One binary and the distribution's GStreamer packages.

```sh
sudo apt install gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-libav

sudo useradd --system --home /var/lib/godwinmix --shell /usr/sbin/nologin godwinmix
sudo install -d -o godwinmix -g godwinmix /etc/godwinmix /var/lib/godwinmix /var/lib/godwinmix/media
sudo install -m 0755 target/release/godwinmix /usr/local/bin/godwinmix
sudo ln -sf /usr/local/bin/godwinmix /usr/local/bin/gmx

godwinmix --example-config | sudo tee /etc/godwinmix/godwinmix.toml > /dev/null
printf 'GODWINMIX_TOKEN=%s\n' "$(head -c 32 /dev/urandom | base64)" \
  | sudo tee /etc/godwinmix/env > /dev/null
sudo chmod 0600 /etc/godwinmix/env

sudo install -m 0644 deploy/systemd/godwinmix.service /etc/systemd/system/
sudo systemctl enable --now godwinmix
```

The unit is [deploy/systemd/godwinmix.service](../../deploy/systemd/godwinmix.service)
and every line in it has a comment saying why it is there.

## Then, in order

1. **Set the token.** `GODWINMIX_TOKEN` in the environment, not `[control]
   token` in the config, so the secret is not in a file that gets committed.
   Without it, whoever reaches port 8080 can take a source, add an output
   pointing at their own server, and shut the mixer down.
2. **Bind to loopback and put TLS in front.**
   [Put it behind a reverse proxy](reverse-proxy.md). The mixer speaks plain
   HTTP and will not terminate TLS itself.
3. **Check what is listening.** `ss -ltnp`. Nothing should be on `0.0.0.0` that
   you did not mean. A published Docker port bypasses ufw on most systems,
   which is why every port in the compose file is published as
   `127.0.0.1:PORT:PORT`.
4. **Check the codecs.** `godwinmix --probe`, or
   `docker exec godwinmix godwinmix --probe`. On a server you control, pin
   `[hardware] encode` to the backend you expect instead of leaving it `auto`,
   so a missing driver fails at startup rather than quietly costing you two
   cores.
5. **Decide where the sources come from.** Put the ones that never change in
   the config as `[[sources]]`. Ones added over the API are saved to
   `godwinmix.runtime.toml` beside the config and come back after a restart;
   once that file exists it is the list, and the config's own sources are not
   merged in.

## Check it works

```sh
export GODWINMIX_TOKEN=$(cat /srv/godwinmix/token)
gmx ctl status
```

That prints what is on programme, every source with its state, and every output
with its state. The same thing as JSON:

```sh
curl -s -H "Authorization: Bearer $GODWINMIX_TOKEN" localhost:8080/api/status | jq .
```

## What to monitor

In the order of how much each tells you:

1. `GET /api/status` answers at all. If it does not, the process is gone.
2. Every output's state is `live`. An output `connecting` for over a minute is
   a destination problem, not a mixer problem.
3. The programme source is what you expect. A mixer showing the slate at 10am
   on a Sunday is a dead camera nobody noticed.

A Prometheus `/metrics` endpoint and a `gmx doctor` that names every missing
GStreamer element in one command are planned (roadmap Phase 0). Until they
land, the three checks above and the log are what there is.

## Restarting and upgrading

The systemd unit runs the mixer with `--supervised` and `Restart=always`, and
the container examples set `GODWINMIX_SUPERVISED=1` beside their restart
policy. That is what lets `core.restart`, and the Restart button in the page,
bring the mixer back: it exits and the supervisor starts it again. On a mixer
started by hand, with neither, `core.restart` says so and keeps running. See
[Restart the mixer from the page](restart-the-mixer.md).

With `Restart=always`, `core.shutdown` is a restart too. `systemctl stop
godwinmix` is how to stop it for good.

To upgrade, replace the binary or pull the new image, then restart. The
programme stops for as long as the restart takes, so do it between broadcasts.
Sources and outputs come back from the runtime file.

## See also

[deploy/README.md](../../deploy/README.md) is the reference version of this
page: ports, the token, proxy configurations for Caddy and nginx, firewall
rules, and what to run when something is wrong on a box you cannot see.
