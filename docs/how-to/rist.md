# Send and receive RIST

RIST is the contribution link many broadcasters and their encoder vendors use
between sites: MPEG-TS over RTP, with the receiver asking for lost packets
again inside a buffer both ends agree on. It does the job SRT does, and some
gear (and some playout centres) only speaks RIST.

## Send the programme

1. Press the palette button (the `⌘K` at the top right) and choose
   **Add an output**, then **RIST destination**.
2. Give it a name and the receiver's address, such as
   `rist://203.0.113.10:5004`. The port is even; the receiver uses the one
   above it for its replies.
3. Open **Advanced** and set **Retransmit buffer** to what the receiver is set
   to, if you were given a number. 1000 ms rides out most internet links.
4. Press **Start sending**.

The output shows as connected once the receiver answers. This machine opens no
port for it: it sends, and the receiver listens.

## Receive a RIST feed

Add a source with the address to listen on, in **Streams and feeds**:
`rist://0.0.0.0:5004`. Point the sender at this machine's address and that
port. The source is live as soon as packets arrive.

## What is not here yet

Bonding several links and the Main Profile (encryption, tunnelling) are not
offered; the Simple Profile is.

Every setting is in the
[network plugins reference](../reference/plugins-network.md#ristoutput-and-rist-in).
