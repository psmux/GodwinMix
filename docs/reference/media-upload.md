# Media upload

`POST /api/v1/media/upload?name=<filename>` accepts a raw file body and requires
the `operate` scope. The legacy `/api/media/upload` route uses the same storage
and validation rules. Successful responses keep the existing shape:

```json
{"name":"clip.mp4","path":"/absolute/library/clip.mp4","size_bytes":1234}
```

Names contain one path segment and at most 200 bytes. Leading and trailing
whitespace is trimmed. Hidden names, path separators, colons and control
characters are refused. Extension checks are case insensitive.

| Media | Extensions |
|---|---|
| Video | mp4, mov, m4v, mkv, webm, avi, ts, mpg, mpeg, flv, wmv |
| Audio | mp3, wav, wave, flac, ogg, oga, opus, m4a, aac, aiff, aif |
| Images | png, jpg, jpeg, bmp, gif, webp, tif, tiff |

The library scan uses the same extension list. Stream metadata comes from
GStreamer discovery, so codec availability depends on the installed runtime.

A name collision returns the existing `not in state` RPC error with the
conflicting `name` in its data and instructions to choose a different name.
Neither completed media nor another upload's temporary file is overwritten.
The server exclusively creates a hidden temporary file, saves streamed bytes,
and publishes it with a hard link that atomically refuses an existing target.
Interrupted body reads remove that upload's temporary file and publish nothing.
