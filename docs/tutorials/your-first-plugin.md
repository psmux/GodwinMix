# Your first plugin

In about fifteen minutes you will have a GodwinMix source plugin written in
Python, producing a real picture, and you will watch the frames come out of it.

Three commands on the finished path do not exist yet: `gmx plugin new`, which
will do the copying for you, `gmx plugin add`, which will install the plugin
into a core, and the step that puts your picture on the multiview; this page
will grow its last three minutes when they land, and everything below works
today.

## What you need

* `python3`, version 3.9 or later. Nothing else is imported: no pip, no venv.
* A checkout of this repository, for the template.
* About fifteen minutes.

GStreamer is optional. If you have `gst-discoverer-1.0` you can inspect the file
your plugin produces; if you do not, the plugin still runs and you can check its
size.

## 1. Copy the template (about one minute)

From the root of your checkout:

```sh
cp -r templates/python my-cam
cd my-cam
```

`gmx plugin new --kind source --lang python my-cam` will do this, and will fill
in the placeholders for you. It does not exist yet, so `cp -r` is the command
and the next step is the filling in.

## 2. Fill in the placeholders (about two minutes)

Four placeholders are spread across the template: `{{name}}`, `{{description}}`,
`{{license}}` and `{{author}}`. Replace them everywhere:

```sh
python3 - <<'PY'
import os
fill = {"{{name}}": "my-cam",
        "{{description}}": "Colour bars, as a first plugin.",
        "{{license}}": "MIT",
        "{{author}}": "you"}
for root, dirs, files in os.walk("."):
    dirs[:] = [d for d in dirs if d not in (".git", "__pycache__")]
    for name in files:
        path = os.path.join(root, name)
        text = open(path, encoding="utf-8").read()
        filled = text
        for key, value in fill.items():
            filled = filled.replace(key, value)
        if filled != text:
            open(path, "w", encoding="utf-8").write(filled)
            print("filled", path)
PY
```

```
filled ./gmx-plugin.toml
filled ./README.md
filled ./main.py
filled ./check
filled ./tests/transcript.jsonl
filled ./tests/replay.py
filled ./schemas/source.json
filled ./skills/source/SKILL.md
```

The name has to be a slug: lower case letters, digits and hyphens, starting
with a letter. It becomes the namespace of every id the plugin registers, so
`my-cam` gives you a source called `my-cam/source`.

## 3. Check it (about one minute)

```sh
./check --quick
```

```
checking my-cam with Python 3.9.6
compile                           ok
pyflakes                          skipped (pip install pyflakes)
gmx-plugin.toml                   ok
offline transcript                ok
your picture                      skipped (--quick)
all checks passed
```

The interesting line is `offline transcript`. That spawned your plugin, fed it
a recorded conversation with the core from `tests/transcript.jsonl`, and checked
that it answered every call correctly. No core was running and no socket was
opened.

`--quick` skips `tests/test_picture.py`, which fails on purpose until you write
it. Step 6 comes back to that.

## 4. Drive it by hand (about three minutes)

A plugin is a process. Control goes in on stdin and comes back on stderr, one
JSON object per line. Media goes out on stdout. That means you can be the core
yourself, with a here document:

```sh
{
cat <<'EOF'
{"jsonrpc":"2.0","id":0,"result":{"core":"godwinmix","version":"0.0.0","api_level":1,"api_compatible":1,"canvas":{"width":320,"height":180,"fps":30},"transport":"container","media":"","instance":"cam","provide":"source","params":{"bars":8}}}
{"jsonrpc":"2.0","id":1,"method":"start","params":{"canvas":{"width":320,"height":180,"fps":30},"transport":"container","media":""}}
EOF
sleep 2
echo '{"jsonrpc":"2.0","id":2,"method":"shutdown","params":{"reason":"by hand"}}'
sleep 1
} | python3 main.py > bars.mkv 2> control.jsonl
```

The first line is the core's answer to the `initialize` your plugin sent before
anything else. The second starts it. Then you wait two seconds while frames go
out, and shut it down.

Keep the `> bars.mkv`. Without it you get several megabytes of raw video on your
terminal.

## You should see

```sh
cat control.jsonl
```

```
{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"plugin":"my-cam","version":"0.1.0","api":1,"transports":["container"],"provides":[{"kind":"source","id":"source","transports":["container"],"media":{"video":"raw","audio":"none","alpha":false,"thumb":true},"capabilities":["restart-in-place","health"],"latency_ms":0,"settings":"schemas/source.json","skill":"skills/source/SKILL.md"}]}}
{"jsonrpc":"2.0","method":"log","params":{"level":"info","message":"my-cam at 320x180@30 as 'cam'"}}
{"jsonrpc":"2.0","method":"initialized","params":{}}
{"jsonrpc":"2.0","id":1,"result":{"latency_ms":0}}
{"jsonrpc":"2.0","id":2,"result":{}}
```

That is the whole handshake. Your plugin spoke first, told the core what it
provides, read the canvas out of the answer, said `initialized`, answered
`start` in zero milliseconds, and exited on `shutdown`.

```sh
ls -l bars.mkv
```

```
-rw-r--r--  1 godwin  wheel  5098218 14 Sep 21:36 bars.mkv
```

Five megabytes for two seconds of 320x180. That is what raw video costs: no
encoder ran, in your plugin or in the core.

With GStreamer installed:

```sh
gst-discoverer-1.0 bars.mkv
```

```
Properties:
  Duration: 0:00:00.232333333
  Seekable: yes
  Live: no
  container #0: Matroska
    video #1: Uncompressed planar YUV 4:2:0
      Stream ID: 4001776ac0cc2a613c958e8ffe167d984eb0b33668e31ea4319dfbcf57bdadce/001:001
      Width: 320
      Height: 180
      Depth: 24
      Frame rate: 30/1
      Pixel aspect ratio: 1/1
      Interlaced: false
      Bitrate: 0
      Max bitrate: 0
```

Uncompressed planar YUV 4:2:0 at exactly the canvas the core asked for. The
duration it reports is short because the stream has no index and no declared
length, which is what a live pipe looks like; the file really holds 59 frames.

To watch it:

```sh
gst-launch-1.0 -q filesrc location=bars.mkv ! decodebin ! videoconvert ! autovideosink
```

It prints nothing and opens a window with the bars in it. Swapping
`autovideosink` for `fakesink` decodes the whole file without a window, which is
the version to run over SSH.

## 5. Change the picture (about five minutes)

Open `main.py` and find `draw()`. It is the only function in the file that is
yours. Replace it with a grey ramp that slides sideways:

```python
def draw(canvas, params, pts_ns):
    """A grey ramp that slides sideways at 60 pixels a second."""
    width, height = canvas["width"], canvas["height"]
    cw, ch = (width + 1) // 2, (height + 1) // 2
    shift = int(pts_ns / 1e9 * 60)
    row = bytes(16 + (x + shift) % 220 for x in range(width))
    return row * height + bytes([128]) * (cw * ch * 2)
```

One I420 frame is three planes laid end to end: `width * height` bytes of
brightness, then two colour planes of `ceil(width/2) * ceil(height/2)` each.
Brightness runs 16 for black to 235 for white. 128 in both colour planes is
neutral grey, which is why the whole picture comes out in shades of grey. `draw`
gets the PTS of the frame it is drawing, in nanoseconds, starting at zero, so
anything that moves is a function of `pts_ns`.

Run the check and the handshake again:

```sh
./check --quick
```

```
checking my-cam with Python 3.9.6
compile                           ok
pyflakes                          skipped (pip install pyflakes)
gmx-plugin.toml                   ok
offline transcript                ok
your picture                      skipped (--quick)
all checks passed
```

Drive it exactly as in step 4, into `ramp.mkv` this time:

```sh
gst-discoverer-1.0 ramp.mkv
```

```
Properties:
  Duration: 0:00:00.199333333
  Seekable: yes
  Live: no
  container #0: Matroska
    video #1: Uncompressed planar YUV 4:2:0
      Stream ID: cc37ddcb16ab0c11333603606454ed52c583d0e0552537dda04d9de6fd9c4ec8/001:001
      Width: 320
      Height: 180
```

Same caps, a different picture, and the same 59 frames in two seconds. Play it
the way you played the bars.

Return the wrong number of bytes from `draw` and the plugin stops and tells you
both numbers in a log line, rather than sending a torn frame.

## 6. Write the test (about two minutes)

Now run the check without `--quick`:

```sh
./check
```

```
checking my-cam with Python 3.9.6
compile                           ok
pyflakes                          skipped (pip install pyflakes)
gmx-plugin.toml                   ok
offline transcript                ok
your picture                      FAILED
    FAIL: tests/test_picture.py
      test_the_first_pixel_is_what_you_meant: the top left luma is 16, not the white bar's 235
      test_picture.py has not been written yet. Replace the tests with checks of your own picture, then set DELIBERATELY_FAILING = False.
something failed. Fix it, then run ./check again.
```

That failure is deliberate, and the second half of it is the real message: the
template ships a test that fails so that a green tick means something. Open
`tests/test_picture.py`, replace the two tests with checks of the picture you
actually draw, and set `DELIBERATELY_FAILING = False`.

For the ramp above, the first pixel is 16 rather than 235, so the assertion to
write is `frame[0] == 16`.

## What you have

A directory with a manifest, a settings schema that every GodwinMix surface
renders as a form, a `SKILL.md` an agent reads before using your source, a CI
workflow, and a plugin that produces frames. When `gmx plugin add` lands, that
directory is the whole of what you hand it.

## What to read next

* [Write a source plugin in Rust](../how-to/write-a-source-plugin.md), for when
  Python is not fast enough or you want the zero copy transports.
* [The plugin manifest](../reference/plugin-manifest.md), every key of
  `gmx-plugin.toml` and every rule the validator checks.
* [The plugin protocol](../reference/plugin-protocol.md), every method, every
  parameter and every error code.

If you got stuck, write it down in [the friction log](../friction-log.md). The
time to a first plugin is the score this project keeps.
