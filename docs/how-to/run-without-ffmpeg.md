# Run without FFmpeg

GodwinMix does not need gst-libav, GStreamer's FFmpeg plugin. This page shows
how to check what a machine takes from it, how to run with none, and how to
put libav back in front where you want it. Why it is built this way is in
[explanation/ffmpeg.md](../explanation/ffmpeg.md).

## See what this machine takes from libav

```
gmx doctor
```

Two lines answer it. On a Windows laptop with the full GStreamer runtime:

```
ok    audio encoder      mfaacenc will be chosen; also here: voaacenc, avenc_aac
ok    gst-libav          installed, and only a fallback: every codec has an element outside it.
```

When a codec has nothing but libav, the line names it, for example
`installed, and used for h265 decode (avdec_h265)`. With gst-libav removed and
a codec left with no decoder at all, the line is a warning that names the
codec: `not installed. Nothing here decodes h265, so a source in it will not
open.`

`godwinmix --probe` prints every catalogue entry with whether it is installed,
and which one was chosen for each role.

## Install GStreamer without gst-libav

On Debian or Ubuntu, install the plugin sets and leave out `gstreamer1.0-libav`:

```sh
sudo apt install gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-tools
```

`gstreamer1.0-plugins-bad` brings `voaacenc`, `faad`, `openh264dec` and
`libde265dec`; `gstreamer1.0-plugins-ugly` brings `x264enc` and `a52dec`. If
gst-libav is already there, `sudo apt remove gstreamer1.0-libav` takes it out
and nothing else goes with it. The desktop `.deb` recommends gst-libav rather
than depending on it, so `apt install --no-install-recommends` leaves it out.

For the best AAC encoder, add fdk-aac. It is in Ubuntu's multiverse
(`sudo apt install gstreamer1.0-fdkaac`) and in Debian's contrib, with
non-free enabled for the library underneath. The catalogue ranks it first, so
nothing needs configuring.

The Docker image has a build argument for it:

```sh
docker build -f deploy/docker/Dockerfile --build-arg WITH_LIBAV=0 -t godwinmix:no-ffmpeg .
```

On Windows and macOS the desktop app carries its own GStreamer, gst-libav
included as a fallback. It is used only for the codecs in the last column of
the table in [explanation/ffmpeg.md](../explanation/ffmpeg.md).

## Put libav back in front for one codec

Every libav element has an entry of its own in the catalogue, so a rank
change from the environment moves it:

```sh
# libav's H.264 decoder over openh264: several cameras on the CPU, or interlaced feeds
GMX_CODEC_RANK=h264-software-decode=70 godwinmix

# FFmpeg's AAC encoder over VisualOn's
GMX_CODEC_RANK=aac-avenc=115 godwinmix
```

The same ranks work in a `[codecs]` table in the configuration file; see
[add-a-codec-entry.md](add-a-codec-entry.md). `decodebin` follows the
catalogue, so a raised libav decoder is what opens sources too.

## Check the result

`gmx codec test <entry>` encodes a second and decodes it back with real
elements. An entry that only encodes, such as `aac-vo`, is decoded with the
best software decoder installed for its codec.

```
gmx codec test aac-vo
gmx codec test h264-software-openh264
```

The codec reference lists every entry: [reference/codecs.md](../reference/codecs.md).
