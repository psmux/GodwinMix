# Letting other devices reach the mixer

`network.share` lets phones and other computers on the same network reach the
mixer, or keeps it to the computer it runs on. **Allow other devices** and
**Stop other devices connecting** on the **Help > Open on another device**
card call it; see [Run a show from phones](../how-to/run-a-show-from-phones.md).

| Method | REST | Scope | Destructive | What it does |
|---|---|---|---|---|
| `network.share {enabled}` | `POST /api/v1/network/share` | admin | yes | Restart the mixer on the same port, on every network (`true`) or on this computer only (`false`) |

The address a mixer listens on is fixed while it runs, so this is always a
restart, and the programme is off air for a few seconds. What restarts it
decides how the change is made:

| Started by | What happens | Answer |
|---|---|---|
| The desktop app | The mixer exits with status 76 (on) or 77 (off). The app keeps the choice in `lan.json`, as the menu item does, with the port the mixer is on now, and starts it again there | `restarting: true` |
| A supervisor (`--supervised`), address from the config file | `control.bind` is written to the config file, host `0.0.0.0` or `127.0.0.1`, port kept, and the mixer exits with status 75 to be started again | `restarting: true` |
| A supervisor, address from `--bind` | Nothing: `--bind` wins over the file | `restarting: false`, and why |
| Nobody (started by hand) | Nothing: nothing would start it again | `restarting: false`, and why |

The desktop app tells its mixer who it is with `GODWINMIX_SHELL=desktop`.
Under a station the station answers this method, since it owns the port, and
exits once every show has stopped.

A mixer with no control token is never put on the network: the call is
refused with `-32001` and `data.open: true`, because anyone on the network
would be admin. Taking a mixer off the network is always allowed. Asking for
what is already so answers `restarting: false` and changes nothing.
`dry_run: true` says what would happen without doing it.

```sh
curl -s -H "Authorization: Bearer $ADMIN" -H "Content-Type: application/json" \
  -d '{"enabled": true}' http://127.0.0.1:54576/api/v1/network/share
```

```json
{"restarting":true,"how":"supervised","message":"Phones and other computers on this network can reach the mixer once it is back. It is restarting now: the programme is off air for a few seconds, and this page reconnects by itself."}
```

## Telling the new mixer from the old one

The old process keeps its port while it stops its shows, so a page that
reconnects at once is talking to the mixer that is leaving. `core.info`
carries `started_ms`, when the answering process started; the card waits
until it changes and `tls.urls` names an address other than loopback (or no
longer does), then shows the code.
