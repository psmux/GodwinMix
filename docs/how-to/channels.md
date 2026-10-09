# Take streams from several encoders into one channel

A channel is a named place encoders publish to. The Sunday service has one,
the youth hall has another, and each can take several streams at once: the
main camera, a second camera, a phone in the gallery, a guest in a browser.
One channel has one set of keys and takes its streams by whichever protocols
you switch on for it:

* RTMP, on the mixer's one RTMP port. What OBS, a phone app and most hardware
  encoders speak.
* SRT, on the mixer's one SRT port, with the key as the SRT passphrase. What
  you want for a feed that crosses the internet.
* WHIP, which is WebRTC from a browser or from OBS, on the same port as this
  page. No port of its own.
* RTMPS, RTMP inside TLS, on a port you choose.

Every channel shares those ports. A stream is a stream once it has arrived,
whichever way it came: it can become a source by itself and be sent on to the
platforms like any other.

Everything here is done on the Channels tab. The same things can be done
through the `channel.*` methods in [the channel reference](../reference/channels.md).

## A channel does not send the programme

A channel passes on what an encoder sends it. Its platform tiles under
**Send on to** forward that encoder's stream to YouTube or Facebook; they do
not send the picture this mixer makes. A YouTube tile on a channel nothing is
sending to says **Waits for the stream**, and YouTube gets nothing.

To send the programme, use **Add destination** on the Outputs panel. See
[Stream to YouTube, Facebook or Twitch](stream-to-a-platform.md).

The Channels tab says this itself while a channel has platforms and no
encoder: a note under the tiles explains it and has a **Send the programme to
YouTube instead** button for each waiting platform. That asks for the stream
key once more (a channel never hands a key back), adds the platform under
Outputs, and takes the tile off the channel unless you switch that off. The
form for adding a platform to an idle channel says the same above the key
box, and its **Send the programme instead** button adds the output straight
away with the server and key you pasted, without adding anything to the
channel.

## Before you start

The listener is the ingest plugin. If the Channels tab says nothing is
listening, it says why and offers the fix: install the ingest plugin from the
Plugins page, or switch it on there.

A mixer with the ingest plugin already has one channel the first time it
starts, called Live (`live`), with RTMP on and one key labelled Default key.
So the RTMP port is open from that first start, and the Channels tab opens on
Live with its Connect section unfolded: the server, the stream key and the
full URL, each with a Copy button. Put those in OBS and press Start Streaming;
you do not have to make a channel first. Make more channels when you want
them.

The default is made once. Remove it and it stays removed, at the next start
too. A mixer that had channels of its own before the default existed does
not get one. If the plugin is installed later, from the Plugins page, Live is
made as it installs.

Any other port opens when a channel needs it: switch SRT on and the SRT port
opens, and removing the last channel that uses a port closes it, the RTMP
port included. The line under **Channels** at the top of the tab says which
ports are open and for which channels, for example
`Open ports: RTMP 1935 for live · SRT 9000/udp for sunday-service`. If a port
a channel wants would not open (another program has it), the same line says
so in amber, with the reason.

The ports are 1935 for RTMP and 9000 for SRT unless the ingest plugin's
settings say otherwise (`rtmp_port`, `srt_port`).

## Make a channel

Press **Add Channel**. It is at the top of the Channels tab, next to Add
destination on the Outputs panel, under Streams and feeds in Add a source, and
in the palette (Ctrl+K).

Type a name, say `Sunday service`. The address it will have is drawn under the
box as you type, something like `rtmp://192.168.1.20:1935/sunday-service`.
Press **Create channel**. A new channel takes RTMP; switch the others on in its
settings.

The card that opens next is the one to set an encoder up from. It has the two
boxes OBS asks for under Settings, Stream, with Service set to Custom, and the
whole address for an encoder with one box:

* Server: `rtmp://192.168.1.20:1935/sunday-service`
* Stream Key: `main?psk=` and the key
* Full URL: `rtmp://192.168.1.20:1935/sunday-service/main?psk=` and the key

Each has a **Copy** button. The QR code beside them is the full URL, for a
phone encoder to scan. Press **Done** when you have what you need. The key is
not lost when the card closes: the mixer keeps it sealed, and **Connect** on
the channel shows it again whenever you want it.

## Switch on SRT, WHIP or RTMPS

Open the channel's settings with the gear on its card. Under **Ways in** there
is a switch for each of RTMP, SRT, WHIP and RTMPS. Switch on the ones you want
and press **Save**. The port for each opens as you save, if no other channel
had it open already, and the Connect section on the card shows the address
for each one that is on.

A channel needs at least one way in. Switching RTMP off on a channel whose
encoders use RTMP cuts them off, with a sentence saying why.

## See and copy the address and a key

Every channel card has **Connect** beside the gear. Press it and the card
unfolds. Above the keys:

* **RTMP, SRT, WHIP** (and RTMPS when it is on) choose the protocol the fields
  below are for. With only one switched on there is nothing to choose.
* The address buttons switch every field between the addresses the mixer
  answers on: the network, and `127.0.0.1` for an encoder on the same machine.
  A phone cannot reach `127.0.0.1`, so give it the network one.
* **Stream name** is `main` to start with. Type `main_720p` or `cam2` and every
  field and the QR code follow.

Under that, each of the channel's keys by its label, with what an encoder is
given for the chosen protocol, each with **Copy**:

| Protocol | Fields |
|---|---|
| RTMP, RTMPS | Server (what Livebox and most encoders call the Stream URL), Stream key, Full URL |
| SRT | Server, Stream ID, Passphrase, Full URL |
| WHIP | WHIP URL, Bearer token |

The key is dots and its last four characters until you press **Show** (the
eye) on that key. **Copy** on a hidden key copies the real thing without
showing it. Press **Connect** again to fold the section; the page forgets
every key it was shown.

A stream that is live has a **Copy URL** beside its name, which copies the
full URL it came in on, over the protocol it came in on, with the key that let
it in.

Only an admin sees a key. Every time a key is shown or copied the mixer writes
a line in its log naming the key and who asked, never the key itself.

## Point an RTMP encoder at it

`main` in the key box is the stream's name and can be anything: `main`,
`cam2`, `main_720p`. Give each encoder on the channel a different one. An
encoder that sends its own ladder of renditions sends each as its own stream,
and the mixer takes them all without transcoding any of them.

`?key=`, `?token=` and `?Token=` work as well as `?psk=`, for encoders that
insist on one of those.

## Point an SRT encoder at it

In the encoder, set the mode to caller, the address to the mixer's and the
port to the SRT port, and then:

* **Stream ID**: `sunday-service/main`, the channel and a stream name. The
  access control form `#!::r=sunday-service/main,m=publish` works too.
* **Passphrase**: the channel's key.

As one URL, for ffmpeg, OBS or `srt-live-transmit`:

```
srt://192.168.1.20:9000?streamid=sunday-service/cam2&passphrase=<the key>
```

The passphrase is the channel's first key. For another of its keys, name it
in the stream id: `#!::r=sunday-service/main,m=publish,u=<the key's id>`. An
encoder that cannot set a passphrase can put the key in the stream id
instead, `sunday-service/main?psk=<the key>`, and leave the passphrase empty;
the stream is then not encrypted.

The stream has to be H.264 or HEVC video and AAC audio in MPEG-TS, which is
what every SRT encoder sends. HEVC is carried on as enhanced RTMP HEVC; a
stream in another codec (AV1, MP2 audio) is turned away with a sentence
saying so.

A caller with the wrong passphrase is refused by SRT itself during the
handshake, so the mixer never sees it and cannot log it; the encoder says
the passphrase was wrong.

## Pull a channel's stream over SRT

Another site, a decoder, vMix or OBS can take a channel's live stream from
the same SRT port the encoders publish to. Set it to caller mode with the
access control stream id ending `m=request` and the same passphrase:

```
srt://192.168.1.20:9000?streamid=#!::r=sunday-service/main,m=request&passphrase=<the key>
```

It gets the stream as MPEG-TS, exactly as the encoder sent it, for as long as
it reads. A stream that is not on air yet is refused; try again once the
encoder is publishing. No second port is opened for players.

## Publish from a browser or OBS by WHIP

WHIP is on the same port as this page, so there is nothing more to open. In
OBS 30 or later, set Service to WHIP, the server to the WHIP URL from Connect
(`http://192.168.1.20:8080/whip/sunday-service/main`), and the Bearer Token to
the key. A browser WHIP client takes the same two things.

The picture should be H.264, which every desktop browser can send, and is then never decoded. VP8, which some Android browsers send instead, is taken too and encoded as H.264 on the mixer, at some cost in CPU; a client set to
VP9 or AV1 alone is turned away with a sentence saying so. The sound arrives as
Opus and is turned into AAC, the one thing the mixer converts on the way in.

WebRTC's media travels on UDP, on a port from a small range starting at the
ingest plugin's `webrtc_port` (8189 unless set), one per publisher and only
while one is publishing. That is how GStreamer's WebRTC works: it cannot
share one UDP port between publishers.

WHIP needs GStreamer's libnice elements. If they are not installed, the
publisher is refused with a sentence naming the package to install.

## Turn on RTMPS

RTMPS is RTMP inside TLS, for an encoder that sends to the mixer over the
internet and should not send its key in the clear. Switch **RTMPS** on in the
channel's settings and choose the port; 443 is what encoders expect. Below
1024 some systems need the mixer to run as an administrator to open it, and
the Channels tab says so if it cannot.

RTMPS needs a certificate, one for the whole mixer:

* **Upload certificate and key** takes the certificate your certificate
  authority issued (the `.crt` or `.pem`) and its private key (the `.key`),
  both at once. They are checked before they are kept.
* **Make a self signed one** makes one for this machine's addresses. Encoders
  will not trust it until told to: in OBS that is not possible, so use an
  uploaded certificate for OBS and a self signed one for encoders that let you
  turn verification off.

The key is sealed with the channel keys and never shown again. The settings
show the certificate's fingerprint, to compare with what an encoder reports.

## Watch it arrive

Within a second or two of the encoder starting, the card's dot turns green and
says Live, and the stream appears as a row: its name, how it arrived when that
was not RTMP (SRT, WHIP, RTMPS), where it comes from and with which key, its
picture size and frame rate, its codecs, a line of its bit rate over the last
minute or so, how long it has been on, and the mixer source it feeds. The
source is called `sunday-service-main` and is in Sources, ready to put in a
scene or take. A second encoder on `cam2`, by SRT say, appears beside it as
`sunday-service-cam2`, and the card says Live, 2 streams.

The row starts with the stream's picture, a small frame of what the encoder
is sending, whether or not it feeds a source. It is renewed every three
seconds. The picture is made from keyframes alone, so it can be a GOP behind
the stream, two seconds for most encoders.

The numbers are read every two seconds while the tab is on screen and
something is live, and not at all otherwise. The pictures likewise: the
mixer decodes nothing for a picture until the tab asks, and stops ten seconds
after it last did. A script can ask for the same picture with
`channel.thumbnail`, or `GET /api/v1/channels/<id>/streams/<name>/thumbnail.jpg`
for the JPEG itself ([the reference](../reference/channels.md#channelthumbnail)).

Every channel stream is on the monitoring wall too, under Channels, with
how many of its destinations are sending: see
[watch many shows at once](monitor-many-shows.md#channels-on-the-wall).

## See every channel on one line

With more than two or three channels the cards get long. **Rows**, beside
**Cards** at the top of the Channels tab, puts each channel on one line
instead. Someone coming from Livebox will know it as the bulk channel
settings table. A line says:

* the dot, green when live, hollow when the channel is switched off;
* the name, and Live (Live, 2 streams with more than one), Waiting for an
  encoder, or Switched off;
* the first live stream's picture size, frame rate and bit rate, as
  `1920×1080 30 fps 4.1 Mb/s`;
* how long it has been publishing, counting up each second;
* how many of its push destinations are sending, as `3 of 4 sending`, with a
  small ring for each one in the colours the tiles use. Record and Watch link
  count among them. The count turns red while one is retrying or has
  stopped; hover it to see which one and why.

Press a line to go back to the cards with that channel's card in view.

The choice between Cards and Rows is kept in this browser, so a laptop can
show rows while a phone shows cards. Rows reads the same numbers the cards do,
on the same two second reading, so it costs the mixer nothing extra. On a
phone the Channels tab is under **Outputs**, and each line folds onto two or
three lines to fit.

The palette (Ctrl+K) finds the tab as **Open Channels**, and also when you
type the words other products use: push destination, restream, stream key,
stream URL, channel dashboard. **Channels as rows** comes up for bulk.

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
* **Ways in**: RTMP, SRT, WHIP and RTMPS, as above.
* **How encoders give their key.** Key in the address (`main?psk=KEY`), or Key
  is the stream name, for a hardware encoder with one box for the key and
  nothing else. The stream is then named after the key's label. For SRT the
  key is then the stream id's stream (`sunday-service/<key>`) and there is no
  passphrase; for WHIP it is the last part of the URL.
* **Keys**, by label and last four characters, each with **Revoke**.
* **Remove channel**, which asks once.

The switches are kept when you press **Save**. A key revoked is revoked at
once, over every protocol.

## When an encoder is turned away

The encoder's own error box says why, in a sentence: the channel name is
wrong, the channel is switched off, it does not take that protocol, the key is
missing, the key is not one of the channel's keys, or somebody else is already
publishing that stream name. The same sentence is in the mixer's log, without
the key, and pops up on the Channels tab while it is open. An SRT caller is
refused with SRT's own access control codes (1401 for a key, 1403 for a
channel that is off or does not take SRT, 1404 for no such channel, 1409 for a
stream name already live), which an encoder shows as its reason.

A name counts as taken only while its publisher is sending. An encoder whose
network dropped, or a browser that was closed without stopping, leaves its
session behind; once that session has sent nothing for 2 seconds, the next
publisher with a valid key takes the name over and the old session is cut off.
One that arrives sooner waits out the 2 seconds rather than being turned away.
See [the reference](../reference/channels.md#a-publisher-that-went-away-without-hanging-up).

## Where to go next

* [Send a channel on to YouTube, Facebook or Twitch](restream-a-channel.md)
* [Record a channel and share a watch link](record-a-channel.md)
* [The channel reference](../reference/channels.md), every method and field
* [Receive a phone or an OBS stream](receive-a-phone-or-obs-stream.md), for one encoder on a port of its own
* [Install a plugin](install-a-plugin.md)
