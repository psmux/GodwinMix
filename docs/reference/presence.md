# Presence: client ids, `presence.list`, `presence.set` and `event/presence.changed`

Who is connected to `/rpc`, from what, and which scene each says it is editing.
For the walk through, see
[Operate with several people](../how-to/operate-with-several-people.md).

## Client ids

Every caller has a client id as well as a token. The token says what it may do;
the client id says which device or connection it is.

| Caller | Client id |
|---|---|
| a `/rpc` connection opened as `/rpc?client_id=phone-cam` | `<token>.phone-cam` |
| a `/rpc` connection with no `client_id` | `<token>.s<n>`, a name the core makes up |
| any call carrying `client_id` in its params (the envelope) | `<token>.<client_id>` |
| an HTTP call without one | the token id alone |

A name is 1 to 32 characters of `a` to `z`, `0` to `9` and `-`. Anything else
is refused with `-32602`, `data.client_id` and `data.pattern`; on the `/rpc`
URL the upgrade itself answers `400` with the same body.

The client id is what `event/scene.patch` carries as `source_client`, what owns
an undo stack, a `scene.history.mark` group, a transaction and a draft, and
what `presence.list` lists. `core.subscribe` answers with it:

```json
{"seq": 120, "events": ["scene.*", "presence.*", "flush"], "ignored_ext": [],
 "client_id": "default.phone-cam"}
```

and so does `core.info`, as `client_id` beside `token`.

## `presence.list`

Scope `read`. `GET /api/v1/presence/list`. No params.

```json
{"clients": [
  {"client_id": "default.t4k2x9q", "token": "default", "device": "Windows Edge",
   "scene": "0192f3c1-…", "since_ms": 1791196800000, "you": true},
  {"client_id": "default.phone-cam", "token": "default", "label": "Sam's phone",
   "device": "iPhone Safari", "since_ms": 1791196811000}
]}
```

| Field | What it is |
|---|---|
| `client_id` | as above |
| `token` | the token id it connected with |
| `label` | the name it gave itself with `presence.set`, else its token's label when the token has one; absent when neither |
| `device` | a guess from the User-Agent: `iPhone Safari`, `Android Chrome`, `Windows Edge`, `gmx CLI`; empty when none was sent |
| `scene` | the id of the scene it says it is editing; absent when it said none |
| `since_ms` | when it connected, milliseconds since the Unix epoch |
| `you` | true on the caller's own entry |

Oldest connection first. Only `/rpc` connections are listed: an HTTP call has
no connection to list.

## `presence.set`

Scope `read`, not mutating: it changes nothing on air and is not in the session
log. `POST /api/v1/presence/set`.

| Param | What it does |
|---|---|
| `scene` | the scene you are editing, by id or name; null or omitted for none |
| `label` | a name for this device; omitted keeps the one it has, `""` clears it |

Answers with the whole list, as `presence.list` does. The scene is stored as
its id whatever it was named by. An unknown scene is `-32004` with the names
that exist. Sent from a caller whose client id has no `/rpc` connection, it is
`-32004` with `data.client_id`: send it over the socket it describes, or pass
that socket's name as `client_id`.

## `event/presence.changed`

Sent when somebody connects, disconnects, or calls `presence.set`. Carries the
whole list, as `presence.list` answers it, plus `seq`, and is ended by
`event/flush` like every other batch. Several changes at once arrive as one.

Subscribe to it by name (`presence.changed` or `presence.*`), or with no
patterns at all. A client that subscribes is sent the current list straight
after its `event/snapshot`.

Nothing is worked out for presence while nobody asks. A connection taking its
seat and giving it back is a map insert and a map remove; the list is built
only for `presence.list` and for a subscriber being told it changed, and a
change is announced only while some connection is subscribed.
