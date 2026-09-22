# Upload a local media file

A browser file picker reads files from the operator's computer. The running core
needs its own copy, so upload the selected file to the media library before
passing the returned `path` to `source.add`. This also works when the browser
and core run on different machines.

Use `POST /api/v1/media/upload?name=clip.mp4`, with the file bytes as the request
body and the usual operator authentication. The response contains `name`,
`path` and `size_bytes`. The existing client `upload` helper handles streaming
and upload progress for browser integrations.

The library accepts common video files, audio files and still images. Decoding
uses the installed GStreamer plugins. An accepted filename does not guarantee
that a damaged file or an unavailable codec can be played.

Uploads never replace an existing file. If the selected name already exists,
or another upload is using it, choose a different name and retry. The server
also refuses a collision created while an upload is in progress. A partial
upload remains hidden until all bytes have been written and saved.

You never have to make the media folder (`[media] dir`, `media` beside where
the mixer runs by default). The mixer makes it when it starts, and again the
next time `media.list` finds it gone, when the answer carries `"created": true`
once. Only a folder it cannot make comes back as an `error`, with the reason.

Uploads require `allow_upload = true` under `[media]` and must fit within
`max_upload_bytes`. The media directory must support hard links, which are used
to publish a completed file atomically without replacing existing work. NTFS,
APFS and ordinary Linux filesystems support this; choose another library
directory if a removable filesystem refuses publication.
