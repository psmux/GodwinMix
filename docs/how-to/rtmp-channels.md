# Take streams from several encoders on one port

A channel is a named place encoders publish to. The Sunday service has one,
the youth hall has another, and each can take several streams at once: the
main camera, a second camera, a phone in the gallery. They all arrive on the
mixer's one RTMP port, each needs a key, and each live stream can become a
source by itself.

Everything here is done on the Channels tab. The same things can be done
through the `channel.*` methods in [the channel reference](../reference/channels.md).

## Before you start

The listener is the ingest plugin. If the Channels tab says nothing is
listening, it says why and offers the fix: install the ingest plugin from the
Plugins page, or switch it on there. The port is 1935 unless the ingest
plugin's settings say otherwise.

## Make a channel

Press **Add RTMP Channel**. It is at the top of the Channels tab, next to Add
destination on the Outputs panel, under Streams and feeds in Add a source, and
in the palette (Ctrl+K).

Type a name, say `Sunday service`. The address it will have is drawn under the
box as you type, something like `rtmp://192.168.1.20:1935/sunday-service`.
Press **Create channel**.

The card that opens next is the one to set an encoder up from. It has the two
boxes OBS asks for under Settings, Stream, with Service set to Custom, and the
whole address for an encoder with one box:

* Server: `rtmp://192.168.1.20:1935/sunday-service`
* Stream Key: `main?psk=` and the key
* Full URL: `rtmp://192.168.1.20:1935/sunday-service/main?psk=` and the key

Each has a **Copy** button. The QR code beside them is the full URL, for a
phone encoder to scan. When the mixer answers on more than one address (the
network, and `127.0.0.1` for an encoder on the same machine), the buttons above
the boxes switch between them. A phone cannot reach `127.0.0.1`, so give it
the network one.

Press **Done** when you have what you need. The key is not lost when the card
closes: the mixer keeps it sealed, and **Connect** on the channel shows it
again whenever you want it.

## See and copy a key again

Every channel card has **Connect** beside the gear. Press it and the card
unfolds to list each of the channel's keys by its label, with three fields
under each: Server, Stream key and Full URL, each with **Copy**.

The key is dots and its last four characters until you press **Show** (the
eye) on that key. Show puts the key in the fields and draws the QR code of its
full URL. **Copy** on a hidden key copies the real thing without showing it,
which is the one to use with somebody looking over your shoulder. Press
**Connect** again to fold the section; the page forgets every key it was shown,
and the next Show or Copy asks the mixer again.

Above the keys:

* The address buttons switch every field between the addresses the mixer
  answers on, as on the first card.
* **Stream name** is `main` to start with. Type `main_720p` or `cam2` and all
  three fields and the QR code follow, so you can copy the variant each
  encoder needs. On a channel whose key is the stream name there is no stream
  name to type, and the fields carry the key alone.

A stream that is live has a **Copy URL** beside its name, which copies the
full URL it came in on: its own stream name, and the key that let it in.

Only an admin sees a key. Someone signed in with a read only token sees the
labels and the last four characters, and Show tells them they cannot. Every
time a key is shown or copied the mixer writes a line in its log naming the
key and who asked, never the key itself.

## Point an encoder at it

`main` in the key box is the stream's name and can be anything: `main`,
`cam2`, `main_720p`. Give each encoder on the channel a different one. An
encoder that sends its own ladder of renditions sends each as its own stream,
and the mixer takes them all without transcoding any of them.

An encoder with one box for the whole address takes
`rtmp://192.168.1.20:1935/sunday-service/main?psk=<the key>`, which is what the
QR code holds.

`?key=`, `?token=` and `?Token=` work as well as `?psk=`, for encoders that
insist on one of those.

## Watch it arrive

Within a second or two of the encoder starting, the card's dot turns green and
says Live, and the stream appears as a row: its name, where it comes from and
with which key, its picture size and frame rate, its codecs, a line of its bit
rate over the last minute or so, how long it has been on, and the mixer source
it feeds. The source is called `sunday-service-main` and is in Sources, ready
to put in a scene or take. A second encoder on `cam2` appears beside it as
`sunday-service-cam2`, and the card says Live, 2 streams.

The numbers are read every two seconds while the tab is on screen and
something is live, and not at all otherwise.

## When the encoder stops

A source nobody has put in a scene goes away with its stream. One that is in a
scene stays where it is, shows its last picture, and says it is waiting; when
the encoder comes back the picture comes back with it. Either way the
programme carries on: nothing an encoder does can stop it.

## The channel's settings

The gear on the card opens them:

* **Name.** The address keeps its first slug, so encoders already set up keep
  working.
* **Take encoders.** Off turns every publisher away.
* **Put each live stream in Sources.** On, a stream called `main` becomes the
  source `sunday-service-main` while it is live.
* **How encoders give their key.** Key in the address (`main?psk=KEY`), or Key
  is the stream name, for a hardware encoder with one box for the key and
  nothing else. The stream is then named after the key's label.
* **Keys**, by label and last four characters, each with **Revoke**. Type who a
  new one is for and press **Make a key**; it opens on the same card the first
  one did, and is under **Connect** from then on.
* **Remove channel**, which asks once.

The switches are kept when you press **Save**. A key revoked is revoked at
once.

## Give a key to one more person

Make a key for each encoder, with a label saying who has it, rather than
sharing one. **Revoke** asks once, then cuts off whoever is on air with that
key and turns them away from then on; nobody on another key notices.
Switching Take encoders off does the same for every key at once.

**Connect** on the card shows every key again, and **Manage keys** at its foot
opens the settings to make one for somebody else or take one back.

## When an encoder is turned away

The encoder's own error box says why, in a sentence: the channel name in the
server address is wrong, the channel is switched off, the key is missing, the
key is not one of the channel's keys, or somebody else is already publishing
that stream name. The same sentence is in the mixer's log, without the key,
and pops up on the Channels tab while it is open.

## Where to go next

* [Send a channel on to YouTube, Facebook or Twitch](restream-a-channel.md)
* [The channel reference](../reference/channels.md), every method and field
* [Receive a phone or an OBS stream](receive-a-phone-or-obs-stream.md), for one encoder on a port of its own
* [Install a plugin](install-a-plugin.md)
