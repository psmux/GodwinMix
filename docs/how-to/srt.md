# Receive and send SRT

SRT carries a stream over a link that loses packets. It asks for the lost ones
again inside a delay budget you choose, so a 300 ms budget rides out a lot of a
bad hotel connection, a 4G modem in a van, or a contribution feed across the
country.

A larger budget is steadier and adds exactly that much delay to the programme.
That is the only trade in the protocol, and `latency_ms` is where you make it.

This page takes about five minutes.

## The two halves

| Direction | What to use | Where it is |
|---|---|---|
| receiving | `srt/source` | the `srt` plugin |
| sending | `srt/output` | built into the core |

Sending is built in on purpose. The programme is already encoded on the output
tee, so a sidecar sender would add a copy of every frame and a process, for
nothing.

## Receive a stream

```sh
dev/harness/stage-plugins.sh
gmx plugin add ./plugins/srt
gmx source add guest --type srt/source --params '{"port":9000,"latency_ms":300}'
```

The source now waits on **UDP** port 9000. Tell the person sending:

```
srt://<the mixer's address>:9000    caller mode, latency 300
```

Both ends should use the same latency. SRT negotiates the larger of the two, so
a mismatch is not an error, just a surprise.

Then take it:

```sh
gmx take guest
```

## Dial out instead of waiting

When the far end is the one waiting:

```sh
gmx source add feed --type srt/source \
  --params '{"mode":"caller","host":"203.0.113.10","port":9000,"latency_ms":300}'
```

An address pasted from a hardware encoder goes in whole and wins over `host` and
`port`:

```json
{"uri": "srt://203.0.113.10:9000?mode=caller&latency=300&streamid=live/cam1"}
```

Anything already in its query string is left alone, so pasting an address never
has a field quietly overwritten behind it.

## Encrypt it

```sh
gmx source add guest --type srt/source \
  --params '{"port":9000,"passphrase":"nine-fat-owls-in-a-row"}'
```

SRT requires 10 to 79 characters and both ends must use exactly the same one.
The passphrase is `format: secret`: stored encrypted, never read back, and never
put in an address this plugin builds, so no log line can carry it.

A mismatched passphrase fails the handshake with no packets and no error the
receiving end can see. It is the usual cause of "nothing arrives", and it is
worth checking second, after the sender is definitely running.

## Send the programme over SRT

```sh
gmx output add away --type srt/output --params '{"uri":"srt://203.0.113.10:9000","latency_ms":300}'
```

Options ride in the query string, the way every other tool spells them:

```
srt://203.0.113.10:9000?mode=caller&latency=300&passphrase=nine-fat-owls-in-a-row
```

To have the far end dial in to the mixer instead, use `?mode=listener` and give
them `srt://<the mixer's address>:9000`.

## Try both ends on this machine

```sh
gmx source add loop --type srt/source --params '{"port":9000}'
gmx output add away --type srt/output --params '{"uri":"srt://127.0.0.1:9000?mode=caller"}'
```

or, with no mixer at all:

```sh
dev/harness/publish.sh srt 9000 &
```

which publishes SMPTE bars and a 440 Hz tone to a listener on port 9000.

## Read the link

```sh
gmx ctl status
```

Once packets are arriving the source's health is `ok` with the link numbers:

```
rtt 14.2 ms, 31 lost, 4 retransmitted, 3.1 Mbit/s link
```

A steadily rising **retransmitted** with a stable picture means the latency
budget is doing its job: packets are being lost and recovered in time. A rising
**lost** means the budget is too small for the link. Raise `latency_ms` and
`gmx plugin reload srt`.

The field names SRT publishes move between GStreamer versions, so several
spellings are tried and anything this build does not publish is left out rather
than reported as a zero.

## Choosing a budget

| Link | `latency_ms` |
|---|---|
| a LAN, a cable between two rooms | 125 |
| a good internet connection, same country | 200 to 300 |
| a 4G modem, hotel wifi, across an ocean | 500 to 1000 |

Start at four times the round trip time the health reports and go up until the
picture stops breaking.

## When nothing arrives

1. Health says no packets have arrived at all. Check the sender is running.
2. The passphrases differ. This is the usual cause and it is silent.
3. `mode` must be the opposite of the far end's. Two listeners never meet.
4. SRT is **UDP**. A firewall rule for TCP on the same port does nothing.
5. A `stream_id` set here refuses a sender publishing under a different one.
   Clear it to accept any.

## What SRT does not do

There is no way for a receiver to ask a sender for a keyframe, so `srt/source`
does not declare `keyframe-request` and the core falls back to its own encoder's
GOP. A source that starts with a grey picture is waiting for the sender's next
keyframe; shorten the keyframe interval at the sending end if that wait is too
long.

## Where to go next

* [Receive a phone or an OBS stream](receive-a-phone-or-obs-stream.md)
* [Send the programme to a WHIP endpoint](send-to-whip.md)
* [The network plugins, every setting](../reference/plugins-network.md)
