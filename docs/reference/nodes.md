# Nodes

A node is another machine running `godwinmix node`, hosting plugins for a
core. This page is the interface: the methods, the config keys, the
transports, the clocks, the metrics, and what breaks.

To get one running, read [Add a second machine](../how-to/add-a-node.md)
first. To understand why a remote plugin looks local, read
[One plugin, three placements](../explanation/one-plugin-three-placements.md).

## The shape

```
 core                                     node studio-b
 +-----------------------+                +-----------------------+
 | programme clock       |  UDP 8447      | GstNetClientClock     |
 | GstNetTimeProvider    | ------------>  | (calibrated, synced)  |
 |                       |                |                       |
 | node bridge, mTLS     | <=== WSS ====> | one socket, every     |
 |   one peer per node   |   TCP 8443     | instance multiplexed  |
 |                       |                |                       |
 | receive, decode,      | <--- RTP ----  | plugin, encode, send  |
 | normalise like any    |   or SRT       | (unixfd to the plugin |
 | other source          |   8500 upward  |  exactly as the core  |
 +-----------------------+                |  would)               |
                                          +-----------------------+
```

The node always dials the core. The core never dials the node, which is what
lets a node sit behind NAT and what makes the core the machine that needs a
stable address.

## Methods

| Method | Scope | Destructive | What it does |
|---|---|---|---|
| `node.list` | read | | Every node: online, offline, or expected |
| `node.get {id}` | read | | One node in full |
| `node.enrol {name, address?, ttl_secs?}` | admin | | Mints a one time token; returns a task handle |
| `node.remove {id}` | admin | yes | Forgets a node and revokes its credentials |
| `node.discover {timeout_ms?}` | read | | mDNS browse for `_godwinmix._tcp` |

`node.enrol` answers with a task, in the shape every call that may take longer
than five seconds answers with (09 section 5 item 9). The handle carries the
token and the command to run immediately; the task itself finishes when the
node dials in, so `task.get` is "has it arrived yet".

### What `node.get` answers

```json
{
  "name": "studio-b",
  "state": "online",
  "address": "10.0.0.21:51422",
  "identity": "spiffe://godwinmix/node/studio-b",
  "version": "0.2.0",
  "platform": "linux-x86_64",
  "heartbeat_age_ms": 412,
  "clock_offset_ms": 0.18,
  "clock_jitter_ms": 0.04,
  "clock_synced": true,
  "provides": ["ndi/source", "screen/source"],
  "plugins": [{ "name": "ndi", "version": "1.2.0", "provides": ["ndi/source"] }],
  "instances": [{ "instance": "cam1", "state": "running", "latency_ms": 82 }]
}
```

`state` is one of three:

| State | Means |
|---|---|
| `online` | A heartbeat arrived inside the last three seconds |
| `offline` | It has connected before and is not answering now |
| `expected` | It is in `[nodes]` in the config and has never connected |

The difference between `offline` and `expected` is worth having: one is a
network fault and the other is a configuration mistake.

## Config

```toml
[nodes]
listen = "0.0.0.0:8443"
clock_port = 8447
clock = "net"
server_names = ["mixer.local", "10.0.0.1"]
advertise = false

"studio-b"    = { address = "10.0.0.21:8443" }
"graphics-pc" = { address = "10.0.0.22:8443", clock = "ptp", transport = "rtp" }
```

| Key | Default | What it is |
|---|---|---|
| `listen` | none | Where the node bridge listens. Absent and with no nodes listed, nothing starts |
| `clock_port` | 8447 | UDP port the programme clock is offered on |
| `clock` | `net` | `net` or `ptp`, offered to every node that does not override it |
| `server_names` | localhost, 127.0.0.1, this machine's hostname | Every name a node might dial the core by. They go in the core's certificate, and a name that is not in it fails the TLS handshake |
| `advertise` | false | Advertise the core over mDNS |

Any other key in the table is a node name. Each one takes `address` (where it
is, for the record: the node still dials), `clock` (`net` or `ptp` for that
machine), and `transport` (the default for sources on it).

Nothing runs unless asked: a core with no `[nodes]` table makes no certificate
authority, opens no port and puts no clock on the network.

### On a source

```toml
[[sources]]
id = "cam1"
type = "ndi/source"
place = "node:studio-b"
transport = "srt"
latency_ms = 150
params = { name = "CAM 1" }
```

| Key | Values | What it is |
|---|---|---|
| `place` | `core`, `in-process`, `sidecar`, `node:<name>` | Where the instance runs |
| `transport` | `rtp`, `srt`, `whip` | How its media reaches the core. Only read for a `node:` placement |
| `latency_ms` | any | The budget, answered on the LATENCY query |

`place` also exists on `[[outputs]]` and `[[filters]]`. A remote output is
cheap: the core already has encoded programme on the tee, so the node receives
it and runs only mux plus sink, and the outage buffer stays on the core's side
so a slow remote destination cannot apply backpressure to the encoder. Remote
filters are permitted and discouraged: a round trip adds two encodes, two
decodes and two network latencies to the source it filters.

A placement the plugin did not declare is refused with error `-32005`, and
`data.placements` lists what it did declare.

## Transports

| `transport` | What it is | Default budget | Use it for |
|---|---|---|---|
| `rtp` | RTP over UDP, `rtpbin` with `ntp-time-source=clock-time` and `ntp-sync` | 200 ms | A LAN. The receiver knows the sender's timeline from the first packet |
| `srt` | One MPEG-TS over SRT, ARQ, latency negotiated as the larger of the two peers | 120 ms | A link that loses packets |
| `whip` | WebRTC | 250 ms | A WAN, or a NAT nobody controls |

For RTP the core listens on two consecutive UDP ports from 8500 and the node
sends to them; the jitter buffer belongs at the receiver and the receiver is
the core. For SRT the node listens and the core dials, because one listener
serves a core that reconnects without the node having to be told.

Raw video never crosses. 1080p30 I420 is 93 MB/s per stream. Between hosts the
node encodes once with its own catalogue entry and the core decodes once on its
usual hardware aware path: one encode and one decode per remote source, which
is the same price a browser source pays today.

The thumbnail is made on the core from the received feed. Nothing sends a
second stream for a picture.

## Clocks

The core runs a `GstNetTimeProvider` on the programme clock. Every node runs a
`GstNetClientClock` against it and waits for `synced` before it starts a single
plugin: a source whose timestamps are on the wrong timeline is worse than a
source that is a second late.

`clock = "ptp"` asks for `GstPtpClock` instead, which wants a wired LAN, a
network card that will timestamp, and the `gst-ptp-helper` installed. A machine
that will not do PTP falls back to the net clock with a warning rather than
refusing to start.

`pipeline.clock` reports the programme clock, its base time, every pipeline in
the process, and one row per node with its offset and jitter. Those two numbers
also arrive on every heartbeat and go straight onto `/metrics`.

## Metrics

| Metric | Labels | What it is |
|---|---|---|
| `gmx_node_clock_offset_ms` | `node` | How far that node's clock sits from the programme clock |
| `gmx_node_heartbeat_age_ms` | `node` | Milliseconds since its last heartbeat |

A heartbeat age climbing past 3000 is a node the core has given up on. That is
the number to alert on.

## Security

* **Enrolment.** One time token, minted by `gmx node token`, bound to one node
  name, expiring in an hour by default. The token carries the first sixteen hex
  digits of the core's certificate authority in front of it, so the node checks
  the core before it hands the secret over. Used once, it is refused with the
  time it was spent.
* **Mutual TLS.** The core is its own certificate authority: a key and a self
  signed root under its runtime directory at mode 0600, and one certificate per
  node with a SPIFFE style identity (`spiffe://godwinmix/node/studio-b`). Both
  sides trust that root and nothing else. The node's name comes out of the
  certificate it presents, not out of anything it says, so a node cannot claim
  to be another one.
* **Rotation** is by reissue: mint another token, enrol again, and the new
  certificate replaces the old one. Certificates are good for a year.
* **Revocation** is `node.remove`. There is no CRL and there does not need to
  be, because the core is the only relying party.
* **Per instance tokens.** Every plugin instance gets a token in `GMX_TOKEN`
  scoped `plugin`, carrying the plugin's name and, for a remote instance, the
  node's. It reaches that plugin's own tools and nothing else: `program.take`
  on one is refused with `-32002`. A node leaving revokes every token minted
  for an instance on it.
* **Secrets.** A settings field the schema marks `"format": "secret"` is
  encrypted at rest with AES-GCM under a key made on first run, kept at mode
  0600 under `~/.godwinmix/secrets`. It is never returned to any surface:
  `plugin.settings.get` answers with the sentinel `"__secret__"` where one is
  set, and writing the sentinel back means "leave it alone". The key is on the
  same disk as the ciphertext, so this keeps live credentials out of a config
  file in a git repository, a support bundle and a backup. It does not stop
  somebody who already has the machine.

A packet capture proving the handshake is mutual is worth doing on the machine
you deploy on: `tcpdump -i any -w node.pcap 'tcp port 8443'` while a node
connects, then look for a Certificate message from the client in the
handshake. The test suite asserts the same thing from inside, by reading the
identity off the presented client certificate.

## The reconciler

The core holds what the operator asked for; each node reports what it actually
has. Four times a second the difference becomes a decision.

```
 desired (store)                        actual (reported)
 sources:                               node cam-room:
   cam1  ndi/source  node:cam-room        cam1  running  latency 82 ms
   cam2  ndi/source  node:cam-room        cam2  failed   "sender not found"
   score browser     node:graphics-pc   node graphics-pc:
                                          (no heartbeat for 4 s)

 decisions this tick:
   cam2   restart on cam-room, attempt 2 of 3 free, then backoff 30 s
   score  mark failed, hold freeze frame, alert "graphics-pc unreachable"
   cam1   nothing
```

Three restarts are free, then 30 seconds doubling to 300, cleared on the first
frame or on removal: the policy the plugin supervisor already had, generalised.
It never blocks the programme. The reconciler produces a list and something
else acts on it, so a node on the other side of a cut cable costs one tick and
not one frame.

## What breaks, and the answer

| What happens | What you see | What to do |
|---|---|---|
| The node's network goes | Its sources hold the freeze frame, then the slate after 45 seconds, and an alert names the node | Nothing. It restores itself when the node comes back, with no core restart |
| The node's process dies | The same | Run it under a supervisor with `Restart=always` |
| The core restarts | The node reconnects on its own, backing off from one second to thirty | Nothing |
| A node enrols with a used token | Refused, naming when it was spent | Mint another |
| The core's certificate does not name the address dialled | The TLS handshake fails, saying so | Add the address to `server_names` and restart the core |
| Two nodes with the same name | The second one's certificate wins the connection and the first is closed | Give them different names |
| The clock will not sync | The node refuses to start a source and says so | Open UDP 8447 between them |
| A plugin only on the core, placed on a node | Refused, naming what the node does have | Install the plugin on the node |
| `gst-plugins-bad` missing on one side | The transport is refused by name at `source.add` | Install it, or write `transport = "rtp"` |

## What is not here yet

Written down so nobody has to find out by trying it:

* A **remote service, device or transition**. The supervisor holds singletons
  as a concrete type rather than behind a trait, so only sources are placed on
  a node today. The bridge carries the calls already.
* A **remote output or filter** built from `place`. The config key parses and
  is validated; the host fork for them is not written.
* **`source.set {place}` as a pad property change.** Today the move is a
  removal and an addition on the mixer's one queue, and the compositor covers
  it: the programme keeps its frame rate and the source holds its last picture.
  A true pad swap needs one more mixer command.
