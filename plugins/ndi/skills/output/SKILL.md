---
name: ndi-output
description: Announce the GodwinMix programme on the studio network as an NDI sender, so anything running NDI can take it with no address to type. Use when the operator wants the programme on the network, mentions NDI out, feeding vMix, OBS, a graphics machine or a second mixer, or wants a return feed for the stage.
---

# ndi/output

The mixer announces its programme on the network under a name. Anything running
NDI can then take it: another mixer, a graphics machine, a laptop at the back
running OBS, a monitor on the stage.

## Adding one

```
output.add {id: "network", type: "ndi/output", params: {name: "Studio A Programme"}}
```

The default name is `GodwinMix`. Give it something the people using it will
recognise when there are two mixers on one network.

## What it costs

The core hands an output the programme already encoded, and NDI wants raw
frames, so this output decodes it. That is one decode, and it is the protocol's
price rather than this plugin's: nothing on the wire is in a form NDI takes. On
a machine already tight for CPU, an NDI output is the first thing to question.

## When there is no runtime

NDI's runtime is not ours to ship. Without it this output refuses to start and
the message carries the download page,
https://ndi.video/for-developers/ndi-sdk/.

## Receiving instead

`ndi/source` takes somebody else's sender, and `list_senders` says who is there.
