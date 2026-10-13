# Media library folders and tags

Folders are directories under the media folder (`[media] dir`). Files and
folders are named by their path relative to it, with `/` between names on
every platform: `Sermons/2026/easter.mp4`. Tags live in
`.godwinmix-media.json` in the media folder, keyed by that path.

## Names

A folder or file name must be legal on Windows, macOS and Linux. Refused, with
`data.field` naming the param: an empty name, `.` and `..`, a name starting
with a dot (hidden from the library), any of `< > : " \ | ? *` or a control
character, a name ending with a dot, a name over 255 bytes, and the device names
`CON`, `PRN`, `AUX`, `NUL`, `COM1` to `COM9` and `LPT1` to `LPT9` with or without
an extension. A path may not start with `/` or hold `\`. A link is never
followed, and a path that resolves outside the media folder is refused. Two
names in one folder may not differ only in case, so a library copied to a case
insensitive disk keeps every file.

## Methods

| Method | Scope | Params | Answer |
|---|---|---|---|
| `media.list` | read | `folder` (path, `""` is the top), `subfolders` (bool), `tag` (one, an array, or `"a, b"`; all must match), `query` (words, each in the path or a tag) | `{dir, items, folders, error}`. Each item adds `folder` (absent at the top) and `tags`. `folders` is every folder, sorted |
| `media.tag` | operate | `name` or `names`, then `tags` (replace), `add_tags`, `remove_tags` | `[{name, tags}]` |
| `media.move` | operate | `name` or `names`, `folder` (where to, `""` is the top), `new_name` (one file only), `force` | `{moved: [{from, to}], updated}` |
| `media.folder.list` | read | none | `{folders: [{path, files, total}], tags: [{name, count}]}` |
| `media.folder.add` | operate | `name`: `Sermons` or `Sermons/2026` | `{path, made}`. Missing parents are made. An existing folder, in any case, answers `made: false` |
| `media.folder.move` | operate | `name`, `into` (`""` is the top), `to` (new name), `force` | `{moved, updated}` |
| `media.folder.remove` | operate | `name`, `contents`: `up` or `top`, `force` | `{moved, updated}` |

REST: `GET /api/v1/media`, `POST /api/v1/media/{id}/tag`,
`POST /api/v1/media/{id}/move`, `GET /api/v1/media/folder/list` and
`POST /api/v1/media/folder/add`, `/move` and `/remove`. A file path in `{id}`
has its `/` written as `%2F`.

`media.upload` takes `folder` and `tags` (comma separated) in the query beside
`name`. The folder is made when missing. The answer's `name` is the file's path
in the library.

## What a move changes

A move renames on disk. A clip's converted `.web.mp4` copy goes with it, and so
do the tags of everything moved. In the same call:

* every source whose `uri` or `params` names a moved file is removed and added
  again under its id with its fader and backup rule, which restarts it. A plain
  path, a `file:` URI, `template:` and `html:` addresses are all followed;
* strings under a `params` key in the scenes document (transitions and
  graphics) are rewritten;
* every `.svg`, `.html` and `.htm` file in the library, and every gallery
  item's `graphic.toml`, has a quoted reference to a moved file rewritten.

`updated` lists what changed in words, such as `source intro` or
`file card.svg`, and says so when one part could not be changed.

A file that is on programme now, in the source taken or a source of the scene
on air, or that the ad break is playing, is refused with `-32001` (not in state),
`data.sources` naming what plays it and `data.field` `force`. Pass `force: true`
to move it anyway. A folder may not go into itself or one of its own folders:
refused with `data.field` `into`. `media.folder.remove` without `contents` is
refused with `data.valid` `["up", "top"]`; when anything inside would land on
a name already taken, nothing moves and `data.valid` lists those names.

Every change sends `media.changed`. `name` is the file's path, and empty when
a folder was made, moved or removed.

## Projects

`project.export` lists each clip by its path with its `tags`, and the bundle
carries `media_folders`, every folder including empty ones. `project.import`
makes those folders and adds the tags to clips that are here after the import.
A clip whose name is taken here by a different file keeps its own tags.

## CLI

`gmx ctl media` lists, with `--folder`, `--subfolders`, `--tag` and
`--search`. Subcommands: `folders`, `mkdir`, `move-folder`, `rmdir`, `move` and
`tag`. `gmx ctl upload` takes `--folder` and `--tag`.

## MCP tools

`list_media`, `tag_media`, `move_media`, `list_media_folders`,
`add_media_folder`, `move_media_folder` and `remove_media_folder`.
