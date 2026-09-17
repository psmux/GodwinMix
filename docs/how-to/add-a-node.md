# Add a second machine

A node is another computer running the same GodwinMix binary, hosting plugins
for your core. The camera is plugged into it, the capture card is in it, the
browser source is rendering on it. The core composites and streams; the node
does the work that has to happen where the hardware is.

Five commands. Two on the core, three on the other machine.

## Before you start

* Both machines can reach each other on the network. The node makes the
  connection, so the core is the one that needs a stable address.
* Both are running the same GodwinMix version. `godwinmix --info` says which.
* The core's config has a `[nodes]` table. Without it the core is not
  listening for nodes and nothing runs.

## 1. Tell the core to listen

In `godwinmix.toml` on the core:

```toml
[nodes]
listen = "0.0.0.0:8443"
```

Restart the core. The log says:

```
the node bridge is up; `gmx node token --name <node>` enrols a machine
```

## 2. Mint a token

On the core:

```bash
gmx node token --name studio-b
```

It prints a token and the exact command to run on the other machine. The token
is good for one enrolment and expires after an hour. Carry it over however you
normally carry a password.

## 3. Start the node

On the other machine:

```bash
godwinmix node --core 10.0.0.1:8443 --name studio-b --enrol-token <the token>
```

That is the whole of it. The node enrols, gets a certificate, writes it to
`~/.godwinmix/node/studio-b.json`, and connects. Every connection after this
one uses the certificate; the token is spent and will not work twice.

Leave it running. It reconnects on its own if the core restarts or the network
drops, so a service unit with `Restart=always` is all the supervision it needs.

## 4. Check it arrived

Back on the core:

```bash
gmx node list
```

```
NAME             STATE          BEAT      CLOCK  HOSTING
studio-b         online         412 ms     0.18ms  0 instance(s)
```

`CLOCK` is how far the node's clock sits from the programme clock. On a wired
LAN it settles under a millisecond within a few seconds. If it stays large, or
the node never reaches `online`, see [What breaks](#what-breaks) below.

## 5. Put a source on it

In the core's config, or over the API:

```toml
[[sources]]
id = "cam1"
type = "ndi/source"
place = "node:studio-b"
latency_ms = 150
params = { name = "CAM 1" }
```

`type` is the plugin, installed on the **node**, not on the core. `place` says
which machine runs it. Everything else is the same as a local source, and so is
everything you see afterwards: the settings form, the tools, the health, the
thumbnail, the events. That is the promise, and it is what the rest of this
page is about.

Over the API instead:

```bash
curl -s -X POST localhost:8080/api/v1/sources \
  -H 'content-type: application/json' \
  -d '{"id":"cam1","uri":"ndi/source","type":"ndi/source","place":"node:studio-b",
       "latency_ms":150,"params":{"name":"CAM 1"}}'
```

`gmx ctl source add` carries an id, an address, a `--type` and a name, and has
no flag for `place`, `latency_ms` or a plugin's own settings, so a source on a
node is added over the API. `uri` is required there: the type id goes in it when
a source has no address of its own.

## Moving a source between machines

A source that is already running is meant to move between machines with one
call. That call does not exist on this build: `source.set` takes a source's name
and colour and nothing else, so `place` has nowhere to go. Until it carries
`place` again, moving a source means changing `place` in the config and
restarting the core.

The core side of the move is written and this is what it does. The programme
does not stop and does not change frame rate: the compositor keeps composing at
the canvas rate whatever the sources are doing, and the source's pad holds its
last picture until the new instance's first frame arrives. A plugin that did not
declare the placement is refused, and the refusal lists the placements it did
declare. `dry_run` says what would happen without doing it.

## Finding a node instead of typing its address

On a flat network with multicast:

```bash
gmx node discover
```

That is mDNS. Plenty of networks do not carry it, which is why the static
`[nodes]` table exists and why nothing depends on discovery working:

```toml
[nodes]
listen = "0.0.0.0:8443"
"studio-b"   = { address = "10.0.0.21:8443" }
"graphics-pc" = { address = "10.0.0.22:8443", clock = "ptp" }
```

A node listed here shows in `gmx node list` as `expected` before it has ever
connected, so a machine that is off is visibly a machine that is off rather
than a machine nobody wrote down.

## Removing a node

```bash
gmx node remove studio-b
```

Destructive, and it says so: the bridge closes, the node's certificate stops
working, every token minted for a plugin on it is revoked, and any source
placed on it goes to the slate until you move it or enrol the machine again.

## What breaks

| What you see | What it is | What to do |
|---|---|---|
| `that enrolment token was already used` | Tokens are good once | `gmx node token --name <node>` for another |
| `that enrolment token was minted for the node X` | The `--name` does not match | Start the node with `--name X`, or mint one for the name you want |
| `the TLS handshake with the core failed` | The core's certificate does not name the address the node dialled | Add that address to `server_names` under `[nodes]` on the core and restart |
| The node never reaches `online` | The bridge connected and the heartbeats are not arriving | Check the core's log for `a node's bridge is down` and what it says after it |
| `CLOCK` stays above a few milliseconds | The clock port is blocked | UDP 8447 from the node to the core, both ways |
| A source on the node holds a still picture and then goes black | The node stopped answering | The core holds the freeze frame for 45 seconds, then the slate, and raises an alert naming the node. It comes back on its own |
| `this core has no node bridge` | No `[nodes]` table | Add `listen` and restart |

## What it costs

One encode on the node and one decode on the core, per source. That is the
same price a browser source pays today and it is what buys the machine
boundary: raw 1080p30 is 93 MB/s and does not go over a network.

The latency budget is the other cost, and it is the one you choose.
`latency_ms` on the source is the jitter buffer: 200 ms is the RTP default,
120 ms the SRT default. Two remote cameras with the same budget stay in lip
sync with each other and with a local file, because every sink downstream
delays by the same amount. A budget that is too small drops packets on a link
that jitters; too large and the operator sees the shot late. Start at the
default and lower it only if you can see why.

## Next

* [Nodes reference](../reference/nodes.md): every method, every config key,
  the transports, the clocks.
* [One plugin, three placements](../explanation/one-plugin-three-placements.md):
  why a remote plugin looks local, and what that is worth.
