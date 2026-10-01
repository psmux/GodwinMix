# Receive and send MPEG-TS over UDP and multicast

Broadcast plant moves video around a building as MPEG-TS on multicast groups.
An IRD, a satellite receiver, an encoder or a playout server sends to a group
such as 239.1.1.1 port 5000, and anything on the network that wants the
picture joins that group. The `udp` plugin does both ends: `udp/source` joins a
group (or listens on a port) and takes one program, and `udp/output` puts the
programme on a group or sends it to one receiver.

This page takes about five minutes.

## Before you start

The `udp` plugin ships with GodwinMix. Its tile appears under Streams and
feeds, and its output in Add an output, once it is installed on the mixer.

## Receive a multicast feed

1. Press **Add a source**, open **Streams and feeds**, and choose **Udp**.
2. Put the group in **Address** (`239.1.1.1`) and its port in **Port**
   (`5000`). Or paste the whole address into **Full address**, as VLC or the
   IRD's own settings write it: `udp://@239.1.1.1:5000`.
3. Press **Add**.

The socket is opened when the source starts and closed when it is removed, so
nothing listens on this machine until you ask for it.

The source's status says what is arriving: the rate, how many packets have
been lost, and, when the feed carries several programs, every one of them by
number and name. That line is how you find the program you want:

```
program 1 (News) of 3: 1 (News), 2 (Sport), 3 (Film)
```

To take Sport, set **Program** to `2` when you add the source. The other two
programs are dropped before the mixer sees them, so a 40 Mbit/s multiplex
costs the decoder one program's worth, not three.

### Things a real feed does, and what happens

| The feed | What you see |
|---|---|
| Several programs in one transport stream | the first one, unless **Program** names another; the status lists them all |
| Constant bitrate with null packets | nothing; the stuffing is dropped before the mixer sees it |
| RTP around the TS (SMPTE 2022-2) | nothing; `rtp://` and bare TS are told apart by their first byte, so the scheme you type does not matter |
| A packet lost on the network | a moment of damage in the picture, counted in the status. Never a stall: UDP has no retransmission, so nothing waits |
| More than half a percent lost in a second | the source goes to degraded and an alert says how much |
| The sender stops | the status says how long ago, and the mixer marks the source stalled. The port stays open |
| The sender starts again | the source goes live again by itself, measured after a five second gap. Nothing to press |

### On a machine with more than one network

Multicast is joined on one interface. Without a choice it is the one the
default route uses, which on a machine with a separate media network is often
the wrong one. Open **Advanced** and put the interface's name in **Network
interface**: `en1` on a Mac, `eth1` or `enp3s0` on Linux. One of its addresses,
such as `10.0.0.5`, works too.

### Only from one sender

Source specific multicast accepts a group only from the address you name. Put
it in **Only from sender** under Advanced, or write it in the address:
`udp://10.0.0.9@232.1.1.1:5000`. The group is normally in 232.0.0.0/8, and the
network has to run IGMPv3.

### Choosing streams

**Streams by PID**, under Advanced, keeps only the elementary streams you list
(`256, 258`), for example one audio language out of three. The program's table
is rewritten so the mixer sees only those. Naming PIDs without a program picks
the program they belong to.

## Receive on a port (unicast)

Leave **Address** at `0.0.0.0` and set the port. The sender points at this
machine's address and that port. Two sources cannot share one unicast port;
the second is refused, and says the port is taken.

## Send the programme

1. Open the command palette with the **⌘K** button at the top right, choose
   **Add an output**, and choose **UDP or multicast**.
2. Type where it goes: `udp://239.1.1.1:5000` for a group, or
   `udp://192.168.1.50:5000` for one receiver. Write `rtp://` instead for a
   receiver that wants RTP.
3. Press **Start sending**.

Anyone on the network can now watch with VLC (`udp://@239.1.1.1:5000`). The
programme is already encoded, so the output adds no encode: it remuxes the
mixer's H.264 and AAC into MPEG-TS, seven 188 byte packets to a 1316 byte
datagram.

Under **Advanced**:

| Setting | When to change it |
|---|---|
| TTL (8) | 1 keeps multicast on the local segment. Raise it only for receivers across a router that forwards multicast |
| Network interface | on a machine with a separate media network, by name or by its address |
| Constant bitrate | for a modulator or a hardware decoder that needs a constant rate. Set it at least 10% above the programme's video and audio bitrate together. The stream is padded with null packets, and the sending is capped a little above the rate so a frame leaves spread out rather than in one burst |
| TS packets per datagram (7) | only when a receiver asks for fewer |
| DSCP | when the network prioritises media by DiffServ class; 34 is common for video |

Health for an output can only say bytes are leaving: UDP has no answer from the
far end. Whether anyone receives them, the receiver has to say.

A receiver that closes, crashes or is restarted changes nothing at this end.
The output keeps sending, stays `live`, and its reconnect count stays where it
was; start the receiver again on the same port and it has a picture at the next
keyframe. If an output does stop sending, it stops saying `live` within three
seconds, and once its five second queue has been full for three more the core
starts the udp plugin process again.

## A word about switches and Wi-Fi

A switch without IGMP snooping sends every multicast group to every port. That
works, and floods the network. On Wi-Fi a multicast stream is sent at the
lowest rate the access point has and can take the whole air for a single 8
Mbit/s picture. Send multicast on a wired network with snooping on.

## On Windows

`udp/source` runs on Windows. `udp/output` does not yet: every sidecar output
receives the programme from the core on a FIFO, and Windows has none. The core
says so when you try. Multicast interface selection on Windows has not been
tested; see [the reference](../reference/udp.md).

## See also

* [The udp plugin reference](../reference/udp.md), every setting of both.
* [Receive and send SRT](srt.md), for a link that loses packets and needs them
  back.
