## From the page

Press **Add RTMP Channel**. It is on the Channels tab, next to Add destination
on the Outputs panel, under Streams and feeds in Add a source, and in the
palette (Ctrl+K). Type a name, say `Sunday service`. The address it will have
is drawn under the box as you type, `rtmp://10.0.0.5:1935/sunday-service`, and
Create channel makes it.

The next card is the one to set an encoder up from. It has the two boxes OBS
asks for under Settings, Stream, with Service set to Custom:

* Server: `rtmp://10.0.0.5:1935/sunday-service`
* Stream Key: `main?psk=` and the key

Each has a Copy button. The QR code beside them is the whole address, for a
phone encoder to scan. When the mixer answers on more than one address, the
buttons above the boxes switch between them.

The key is shown once. The mixer keeps only its last four characters, so copy
it before you press Done. For a second encoder press Make another key rather
than sharing the first: one key per encoder means taking one back costs nobody
else anything.

The channel is a card on the Channels tab from then on. Its dot pulses green
while something publishes. Each stream on it is a row with its picture size,
frame rate, codecs, a bitrate line, who is publishing and with which key, how
long for, and the mixer source it feeds. Connect an encoder on the card shows
the server again and makes a new key.

The gear opens the channel's settings:

* Name. The address keeps its first slug, so encoders already set up keep
  working.
* Take encoders. Off turns every publisher away.
* Put each live stream in Sources. On, a stream called `main` becomes the
  source `sunday-service-main` while it is live.
* How encoders give their key: in the address (`main?psk=KEY`), or as the whole
  stream name, for an encoder with only one box.
* Keys, by label and last four characters. Revoke asks once and then cuts that
  key off. Make a key makes another and shows it once.
* Remove channel, which asks once.

The switches are saved with Save. A key revoked is revoked at once.

The panel asks the mixer for channel events only while it is on screen, and
lets go of them when you switch to another tab.
