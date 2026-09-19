# Browser file sources

`sourceFiles(client, options)` in `ui/shell/source-files.js` builds a real
browser file picker, upload progress and per file results. Add its `node` to a
chooser. Its button opens a multiple selection input for video, audio and image
files. The existing public upload endpoint decides which formats the mixer
accepts.

Options are `label`, `accept`, `multiple`, `onAdded(source)` and
`onError(error, context)`. Capture the target scene when opening the chooser,
then use `onAdded` to add each returned source to that scene. This callback is
awaited before the next file is processed. `context` contains the file, any
created source and the failing phase: `upload`, `source` or `scene`.

The returned object exposes `node`, `input`, `button`, `status`, `progress`,
`browse()`, `upload(files)` and `destroy()`. Call `browse()` directly from a
user gesture. `upload(files)` returns results with each file and its source or
error. Repeated calls while a batch is active share the same promise.

Files travel through `client.upload`, which reports the stored mixer path.
The helper sends that path to public `source.add`. It does not use the browser's
local path or guess the server's media directory. Unique upload filenames
protect existing footage; the source keeps the original filename as its label.
Uploads run sequentially. A failed file does not stop the remaining selection.
If scene insertion fails, the created source and uploaded file remain available.

`destroy()` removes the interface. A batch the operator already selected keeps
running and still calls `onAdded` for the captured scene. Upload failures use
the caller's `onError`, or a window toast when no handler was supplied.
