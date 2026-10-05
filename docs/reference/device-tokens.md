# Device tokens

A device token is a credential an admin makes while the mixer runs, for one
phone or tablet, and takes back with one call. It sits beside the tokens the
config file sets (`control.token`, `GODWINMIX_TOKEN` and the `[[tokens]]`
table) and is checked by the same code: one protocol, whoever is calling.
**Help > Open on another device** in the page is built on these three methods;
see [Run a show from phones](../how-to/run-a-show-from-phones.md).

| Method | REST | Scope | Destructive | What it does |
|---|---|---|---|---|
| `token.create {label?, id?, scope?}` | `POST /api/v1/token/create` | admin | no | Make one. The secret is in this answer and nowhere else |
| `token.list` | `GET /api/v1/token/list` | admin | no | Every device token, without secrets |
| `token.revoke {id}` | `POST /api/v1/token/revoke` | admin | yes | Take one back. The device's next call is refused |

## `token.create`

| Param | Default | What it is |
|---|---|---|
| `label` | `"Phone"` | What a person calls the device. Trimmed, 64 characters at most |
| `id` | made from the label | A slug. When absent, the label's slug, with `-2`, `-3` on the end when that id is taken by any token, configured or device |
| `scope` | `"operate"` | `read`, `operate` or `admin`. The ladder applies: `operate` can read, `admin` can do both |

```sh
curl -sk -H "Authorization: Bearer $ADMIN" -H "Content-Type: application/json" \
  -d '{"label": "Test phone"}' https://192.168.77.173:18431/api/v1/token/create
```

```json
{"created":"2026-10-05T08:58:07.636Z","id":"test-phone","label":"Test phone","scope":"operate","token":"b94wbyphk5p2w79ydfu3tb56p6y59f3rytsd3kk8"}
```

`token` is 40 characters from a 32 letter alphabet, about 200 bits. Present
it like any other token: `Authorization: Bearer <token>`, or `?token=` on a
GET and a WebSocket. To sign a browser in, open the page with the token in
the fragment, `https://<host>:<port>/#token=<token>`. The page stores it,
removes it from the address bar, and never sends the fragment to the server.

A device token carries one scope and nothing else: `confirm` is `none`, the
profile is `standard`, it is not an agent token and has no safety override of
its own. It is a rehearsal token on a mixer started with `--rehearsal` and a
live one otherwise. `program.history` records takes against its `id`.

## `token.list`

```json
{"tokens":[{"created":"2026-10-05T08:58:07.636Z","id":"test-phone","label":"Test phone","scope":"operate"}]}
```

Only device tokens. A token from the config file is not listed, because
nothing over the protocol changes those.

## `token.revoke`

```json
{"revoked":{"created":"2026-10-05T08:58:07.636Z","id":"test-phone","label":"Test phone","scope":"operate"}}
```

From the answer on, the token signs nothing in. A connection that signed in
before the revoke, a page open on `/rpc` for example, is refused on its next
call with `-32002`:

```json
{"code": -32002, "message": "the device token 'socket-phone' was revoked. Ask whoever runs this mixer for a new one, or scan the code under Help, Open on another device, again.", "data": {"token": "socket-phone", "revoked": true}}
```

A new request with it is refused as any unknown secret is, `401 wrong token`.

## Errors

| When | Code | What the message says to do |
|---|---|---|
| the mixer has no token at all | `-32001` | set `GODWINMIX_TOKEN` or `control.token` and restart; on an open port every caller is already admin |
| `scope` is `plugin` | `-32602` | use `read`, `operate` or `admin` |
| `id` is not a slug | `-32602` | the slug it would become, or leave `id` out |
| `id` is taken | `-32602` | pick another id, or leave `id` out |
| `token.revoke` names no device token | `-32004` | the ids there are, with `data.valid`; a `[[tokens]]` entry is removed from the file instead |
| the file cannot be written | `-32603` | the folder that has to be writable. Nothing changed |
| the file on disk does not parse | `-32001` | fix it or move it aside; nothing is written over it |
| the caller is not admin | `-32002` | ask for a token with `admin` |

## Where they are kept

`<config name>.devices.toml` beside the config: `godwinmix.devices.toml` for
`godwinmix.toml`. It holds the id, label, scope, the time it was made and the
SHA-256 of each secret, never the secret itself, so the file in a backup or a
support bundle signs nobody in. It is written whole, through a temporary file
and a rename, and is owner readable only on macOS and Linux.

```toml
[[devices]]
id = "test-phone"
label = "Test phone"
scope = "operate"
created = "2026-10-05T08:58:07.636Z"
sha256 = "1f48182b5d48c51326d42e7cfd4c5b61168cb12e0897a5e9a50211c39f019acc"
```

Under a station (see [Shows and the station](shows.md)) there is one file for
the machine, beside the config the station was started with. The station
answers the three methods itself, so the file has one writer. A call is
checked twice, at the station and again at the show it is relayed to; every
show is started with `GODWINMIX_DEVICE_TOKENS` naming the station's file and
reads it again when it changes. A token made at the station works at the show
at once. A revoke reaches a show within one second; the station itself
refuses the device straight away.

A file that does not parse does not empty the list and is never written
over. The mixer keeps what it last read, logs a warning naming the file, and
answers `token.create` and `token.revoke` with `-32001` until the file is
fixed or moved aside.
