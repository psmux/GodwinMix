# Send a channel on to YouTube, Facebook or Twitch

A channel is a place an encoder publishes to on the mixer. Its stream can be
passed straight on to one platform or several at once, as it arrives, without
going through the mixer's programme. Nothing is decoded or encoded on the
way, so ten destinations cost about what one does.

This is how a service goes to YouTube and Facebook at once from the one
encoder in the rack, while the mixer's own programme goes somewhere else or
nowhere.

The Channels page this describes arrives separately; until it does, the three
methods in [the reference](../reference/channels.md#destinations) do the
same.

## Add a destination

Open the channel and press **Add destination**. Pick the platform. For
YouTube, Facebook and Twitch the server is filled in, and the one thing to
paste is the stream key:

| Platform | Where to copy the key from |
|---|---|
| YouTube | YouTube Studio, Go live, Stream settings. The stream key, not the stream URL |
| Facebook | The Live producer page, Streaming software |
| Twitch | The Creator Dashboard, Settings, Stream. The primary stream key |

**Custom RTMP** takes a server and a key, or a whole address with the key on
the end and the key box left empty. **SRT** takes an `srt://` address and no
key.

Save. The key is never shown again; the row says the platform and whether a
key is there.

If the channel has more than one stream live (an encoder sending its own
1080p and 720p, say), **Stream** picks which one goes. Left as it is, the
first stream to go live is sent.

## Read the row

| The row says | What it means |
|---|---|
| Waiting | on, and nothing is being published to the channel yet |
| Connecting | dialling the platform |
| Live, with a bitrate | sending |
| Reconnecting | the platform went away and is being dialled again |
| Failed | the platform refused the key |

Under a row that is not live is the reason, in a sentence: "nothing answered
at rtmp://10.0.0.9:1935" means the server is down or the address is wrong;
"YouTube refused the key" means paste it again from the platform.

When a platform drops, the others carry on and nothing reaches the encoder.
The one that dropped is dialled again: gently for YouTube, Facebook and
Twitch, which penalise a client that hammers them, and quickly for a server
of your own. When it comes back it starts at the next keyframe, so viewers
see the picture return rather than a smear.

## Change or remove one

**Edit** on the row changes the key, the server, the stream it sends, or
switches it off without forgetting it. A key left empty in the form is kept.
**Remove** stops it and forgets it; the channel and its other destinations
are not touched.
