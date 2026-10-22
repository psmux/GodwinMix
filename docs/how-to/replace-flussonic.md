# Replace Flussonic with GodwinMix

A machine that runs Flussonic for a set of TV channels can run GodwinMix in
its place, from the same `flussonic.conf`. GodwinMix reads the file, runs
each `stream` as a show without compositing, and answers Flussonic's
playback URLs and its `/streamer/api/v3` API on the same port. A gateway, a
cache, a player or a monitoring page that reads Flussonic needs no change.

The order is: check the file, run GodwinMix beside Flussonic on another
port, compare, switch, and keep the way back open. Flussonic stays installed
and its config is never rewritten by any of this, unless you edit streams
through GodwinMix's API.

[Reference: the Flussonic facade](../reference/flussonic-facade.md) has every
path and field.

## Install

Install GodwinMix and its ingest plugin as for any headend ([take headend
feeds into direct shows](headend-feeds.md)). The ingest plugin runs the
inputs; without it every stream waits.

The user GodwinMix runs as needs to read `/etc/flussonic/flussonic.conf`,
and to write it only if you will edit streams through the API.

## Check the file

```sh
godwinmix --config /etc/godwinmix/godwinmix.toml headend --flussonic /etc/flussonic/flussonic.conf --check
```

It prints what it would make and every line it will not carry, and starts
nothing:

```text
/etc/flussonic/flussonic.conf: 52 streams, 51 converted and 1 copied, 0 off
global, not carried: dvr archive: GodwinMix keeps the live window only
line 12, stream CH117, waiting: rendition fields deinterlace, device, fit, hardware, keyframe_frames, pad_color, tracks are passed on, and this build does not act on them yet
```

`ignored` is a directive or option GodwinMix has no use for. `not carried`
says what GodwinMix does instead. `waiting` names transcoder options that
are passed on for the GPU conversion, which this build does not have yet.

## Run it beside Flussonic

Give it another port, so Flussonic keeps 8090 and both run from the same
multicast:

```sh
godwinmix --config /etc/godwinmix/godwinmix.toml headend \
    --flussonic /etc/flussonic/flussonic.conf --port 8091
```

```text
GodwinMix is running. Open http://127.0.0.1:8080/ in a browser.
Headend: Flussonic's URLs and API on http://0.0.0.0:8091/, from /etc/flussonic/flussonic.conf.
```

GodwinMix's own page is on its control port as always, and shows every
stream as a show. Leave out `--port` and it takes the `http` line's port,
which only works once Flussonic is off.

## Compare

With the `edit_auth` user and password from `flussonic.conf`:

```sh
curl -s -u admin:password localhost:8091/streamer/api/v3/streams | jq '.streams[] | {name, status: .stats.status, kbps: .stats.bitrate}'
curl -s localhost:8091/CH117/index.m3u8
ffprobe -v error -show_streams http://localhost:8091/CH117/index.m3u8
```

Every stream should read `running` within a minute. Point a monitoring page
that reads Flussonic's API at the new port, or a player at
`http://host:8091/<name>/index.m3u8`, and compare it with the same stream on
8090. A stream that does not run says why in `stats.last_error` and in the
log.

To have a log in Flussonic's format for a reader that counts scrambled
inputs and refused GPU sessions, add `--flussonic-log` with a path of its
own while Flussonic still writes its log.

## Switch

Stop Flussonic, and start GodwinMix on the `http` port, as a service so it
comes back after a reboot:

```ini
# /etc/systemd/system/godwinmix-headend.service
[Unit]
Description=GodwinMix in place of Flussonic
After=network-online.target
Wants=network-online.target

[Service]
User=godwinmix
Environment=GODWINMIX_SUPERVISED=1
ExecStart=/usr/local/bin/godwinmix --config /etc/godwinmix/godwinmix.toml headend --flussonic /etc/flussonic/flussonic.conf --flussonic-log /var/log/flussonic/flussonic.log
Restart=always

[Install]
WantedBy=multi-user.target
```

```sh
systemctl stop flussonic
systemctl disable flussonic
systemctl enable --now godwinmix-headend
```

Everything that read port 8090 now reads GodwinMix. Segment names are made
from the time each segment began, so a cache in front never confuses one of
GodwinMix's segments with one of Flussonic's.

## Edit streams

Edit `flussonic.conf` as before, by hand or through the API (`PUT` and
`DELETE /streamer/api/v3/streams/<name>`). GodwinMix notices within a second
and restarts only the streams that changed. A file with a mistake changes
nothing, and the log says which line.

## Go back

```sh
systemctl disable --now godwinmix-headend
systemctl enable --now flussonic
```

Flussonic reads the same file. Edits made through GodwinMix's API keep
every line it did not change, so Flussonic reads them too.

## What is not there yet

* Segments are fragmented MP4. MPEG-TS segments, `tracks-v1a1` with the
  video and the first sound in one stream, and `/<name>/mpegts` come with
  the TS packaging; until then those two paths answer 501.
* One sound track per stream. Every sound track as its own `EXT-X-MEDIA`
  comes with carrying every track of an input.
* MPEG-2 sources need the GPU conversion, which is not in this build: a
  stream whose source is MPEG-2 video waits, and says so.
* No DVR.
