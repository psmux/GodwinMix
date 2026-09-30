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

## Before you start

The listener is the ingest plugin. If the Channels tab says nothing is
listening, it says why and offers the fix: install the ingest plugin from the
Plugins page, or switch it on there.

No port is open until a channel needs it. With no channels the mixer listens
on the port this page is on and nothing else. Make a channel and the RTMP port
opens; switch SRT on and the SRT port opens; remove the last channel that
uses a port and it closes again. The line under **Channels** at the top of the
tab says which ports are open and for which channels, for example
`Open ports: RTMP 1935 for sunday-service · SRT 9000/udp for sunday-service`.
If a port a channel wants would not open (another program has it), the same
line says so in amber, with the reason.

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
| RTMP, RTMPS | Server, Stream key, Full URL |
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

The stream has to be H.264 video and AAC audio in MPEG-TS, which is what
every SRT encoder sends unless told otherwise. A stream in HEVC is turned away
with a sentence saying so.

A caller with the wrong passphrase is refused by SRT itself during the
handshake, so the mixer never sees it and cannot log it; the encoder says
the passphrase was wrong.

## Publish from a browser or OBS by WHIP

WHIP is on the same port as this page, so there is nothing more to open. In
OBS 30 or later, set Service to WHIP, the server to the WHIP URL from Connect
(`http://192.168.1.20:8080/whip/sunday-service/main`), and the Bearer Token to
the key. A browser WHIP client takes the same two things.

The picture has to be H.264, which every browser can send; a client set to
VP8 or VP9 is turned away with a sentence saying so. The sound arrives as
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

## Where to go next

* [Send a channel on to YouTube, Facebook or Twitch](restream-a-channel.md)
* [The channel reference](../reference/channels.md), every method and field
* [Receive a phone or an OBS stream](receive-a-phone-or-obs-stream.md), for one encoder on a port of its own
* [Install a plugin](install-a-plugin.md)
