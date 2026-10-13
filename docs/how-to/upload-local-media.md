# Upload a local media file

A browser file picker reads files from the operator's computer. The running core
needs its own copy, so upload the selected file to the media library before
passing the returned `path` to `source.add`. This also works when the browser
and core run on different machines.

Use `POST /api/v1/media/upload?name=clip.mp4`, with the file bytes as the request
body and the usual operator authentication. The response contains `name`,
`path` and `size_bytes`. The existing client `upload` helper handles streaming
and upload progress for browser integrations.

The library accepts common video files, audio files and still images,
including SVG. A PNG, WebP or SVG with transparency, and a WebM or MOV clip
with an alpha channel, keep their transparency when added as a source; see
[show a picture](show-a-picture.md). Decoding
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

## Put files in folders and tag them

The Media panel has the library's folders down its left side. They are real
folders inside the media folder, so one you make in Explorer or Finder shows up
the next time the list is read, and a file you move there by hand is where the
panel says it is. Folders can sit inside folders as deep as you like. A small
arrow opens and closes a folder that has others in it.

* **All files** shows everything. Pick a folder to see the files right in it,
  and tick **Include subfolders** to see what is under it as well.
* **New folder** makes one inside the folder you are looking at. Right click a
  folder (or hold your finger on it) for **New folder inside**, **Rename**,
  **Move to** and **Remove folder**.
* Upload puts files in the folder you are looking at.
* Drag a file onto a folder to move it there, or drag a folder onto another.
  Drop on **All files** to move something to the top. With a finger, use
  **Move to** on the long press menu instead.
* **Select** lets you tick several files, then **Move to** or **Add tags** for
  all of them at once.
* Right click a file for **Tags**, **Move to** and **Rename**. A tag is any
  word or two: `easter`, `choir`, `youth night`. The box suggests the tags
  already in use. Tags ignore case, so `Easter` and `easter` are one tag, kept
  the way it was first typed.
* Click a tag on a file to show only files with that tag. The search box
  matches file names, folder names and tags.

Moving a file or a folder never copies or deletes anything. Every source that
plays a moved file is pointed at its new place in the same step, and so is a
graphic template or a gallery graphic that names a picture by its path. A
source that has to be pointed somewhere new restarts, so a file on air right
now is refused with the source's name. Take something else to programme first,
or say **Move anyway**.

Removing a folder asks where the files and folders in it go: up into the folder
above, or to the top of the library. Nothing is deleted either way. If a name
is already taken where they would go, nothing moves and the message lists the
names to sort out first.

The same works without the page:

```sh
gmx ctl media mkdir "Sermons/2026"
gmx ctl upload easter.mp4 --folder "Sermons/2026" --tag easter
gmx ctl media move welcome.mp4 --to Sermons
gmx ctl media tag "Sermons/2026/easter.mp4" --add choir
gmx ctl media --folder Sermons --subfolders --tag easter
gmx ctl media rmdir Sermons --contents up
```

Tags are kept in `.godwinmix-media.json` in the media folder, because no file
system has tags that work on Windows, macOS and Linux alike. Copying the media
folder to another machine takes them along. If that file is damaged it reads as
empty, and the next change sets the damaged one aside as
`.godwinmix-media.damaged.json` rather than writing over it.
