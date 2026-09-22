# Restart the mixer from the page

Some settings only take effect when the mixer starts. This page is how to
restart it without a terminal, and what decides whether that is possible.

## Ask whether it can

```sh
curl -s localhost:8080/api/v1/core/info | jq '{supervised, restart}'
```

```json
{ "supervised": true, "restart": { "possible": true, "how": "supervised" } }
```

`possible` is true when something starts the mixer again after it exits. The
mixer cannot find that out by itself, so it is told with `--supervised` or
`GODWINMIX_SUPERVISED=1`. These set it for you:

* the systemd unit in `deploy/systemd/`, which also has `Restart=always`;
* the compose file in `deploy/docker/`, beside `restart: unless-stopped`;
* the desktop app, which starts its own mixer again when it exits asking to be
  restarted.

A mixer you started yourself in a terminal has none of them, and answers
`"possible": false, "how": "none"`.

## Restart it

```sh
curl -s -X POST localhost:8080/api/v1/core/restart \
  -H "authorization: Bearer $GODWINMIX_TOKEN"
```

The token needs the admin scope. On a supervised mixer:

```json
{"restarting": true, "how": "supervised", "message": "The mixer is restarting. The programme is off air until it is back, usually within a few seconds, and this page reconnects by itself."}
```

The mixer closes its outputs and exits with status 75, and the supervisor
starts it again with the same config, the same sources and outputs from the
runtime file, and on the same address. The programme is off air in between.

On a mixer started by hand nothing happens to it, and the answer says so:

```json
{"restarting": false, "how": "none", "message": "Nothing would start this mixer again, because it was started by hand, so it is still running. ..."}
```

Stop it in its terminal and start it the same way, or run it under the unit or
the container.

## In the desktop app

The menu has Restart the mixer. The page, or anything else talking to the
mixer, can call `core.restart` as above: the app sees its mixer exit with
status 75 and starts it again on the same port, so the page reconnects without
being sent anywhere. A Restart button in the page itself, shown when
`restart.possible` is true, arrives in a later change.

## Under systemd

With `Restart=always`, `core.shutdown` brings the mixer back too. Stop it for
good with `sudo systemctl stop godwinmix`.
