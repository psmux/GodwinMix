# The broadcast preset

A contribution feed: SRT in and SRT out, hardware tally over TSL, a Companion surface, and nodes joined with mTLS.

## What it gives you

Two venues arriving over SRT and the studio camera over NDI, going out over SRT with an RTMP backup, with tally lights on the cameras and a Companion surface on the desk.

## What you need

The venues' encoders pointed at this machine's ports 9001 and 9002, an SRT address to deliver to, and a TSL tally device or Companion on the same network.

## Three steps

1. `gmx preset apply broadcast`, or pick it on the welcome page if this machine has a screen.
2. Finish the checklist: press Install for the SRT plugin, so the venue feeds and the network output can connect.
3. Press **Studio** to go on air, then cut to a venue when its feed is green.

## When it does not work

**A venue never connects.** Usually the port is not open, or the encoder is in listener mode too. One end calls and one end listens; these two listen.

**The picture breaks up under load.** Usually SRT latency is too low for the link. Raise `latency_ms` to three times the round trip time and have the venue do the same.

**Tally lights are on the wrong camera.** Usually the tally plugin maps source ids to addresses, and the ids here are `venue-a`, `venue-b` and `studio`. Check that map before the cameras.
