# Project files and the project methods

A project is one mixer's whole setup in one file: its show settings, its
sources, its outputs and their renditions, its channels, its scenes, the page
layout the browser sent along, and the names and sizes of its clips. The page
writes one with File > Save project as and reads one with File > Open project
(see [Save and open a project](../how-to/save-and-open-a-project.md)). Both
are ordinary protocol methods, admin scope, so a script or an agent can do the
same.

When one machine runs several shows, a project is what one show will be.

## `project.export`

Admin scope. `POST /api/v1/project/export`. Changes nothing.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `name` | string | `"GodwinMix project"` | what to call the project |
| `include_secrets` | bool | false | put stream keys, channel keys, destination addresses and the control token in the file |
| `include_media` | bool | false | put the clips themselves in, as base64, rather than only their names and sizes |
| `page` | object | null | anything the caller wants back when the file is opened; the page puts its workspace, its settings and its theme here |

The answer is the file itself. Save it as it is, with the extension
`.gmxproject`.

`include_media` is refused past 256 MB of clips, with the size in the
message and `data.bytes`. Copy the media folder across instead.

## `project.import`

Admin scope. `POST /api/v1/project/import`. Destructive, so a token set to
confirm destructive calls is asked.

| Param | Type | Default | Meaning |
|---|---|---|---|
| `file` | object or string | required | the project, as `project.export` answered it, or its text |
| `mode` | `"replace"` or `"merge"` | `"replace"` | what to do with what this mixer already has |
| `dry_run` | bool | true | answer with what would change and change nothing |
| `machine` | bool | false | also write the file's machine settings |

`dry_run` is true unless `false` is sent. The whole file is read and checked
before anything moves, so a file that is wrong anywhere is refused whole.

**replace**: the file's sources, outputs, channels and scenes become this
mixer's. Anything here that the file does not have is taken out. An entry
that is the same here as in the file is kept running, not restarted. The
file's show settings are written where they differ.

**merge**: everything in the file is added beside what is here. A source,
output or channel whose id is taken arrives renamed (`cam-wide` becomes
`cam-wide-2`), and every scene that draws a renamed source is changed to
match. Scenes get new ids and, where a name is taken, a new name. Show
settings are written only where this mixer's config file sets nothing, the
same rule `preset.apply` follows.

### The answer

```json
{
  "dry_run": true,
  "name": "Sunday service",
  "written_by": "godwinmix 0.2.0",
  "changes": [
    {"part": "setting", "id": "canvas.width", "action": "set", "note": "1920 to 1280"},
    {"part": "source", "id": "cam-wide", "action": "rename", "to": "cam-wide-2"},
    {"part": "output", "id": "youtube", "action": "wait", "note": "the file left out its stream key"},
    {"part": "scene", "id": "Wide", "action": "add"},
    {"part": "channel", "id": "sunday-service", "action": "replace"},
    {"part": "media", "id": "intro.mp4", "action": "missing"}
  ],
  "waiting": ["output youtube is not started: the file was saved without its stream key. ..."],
  "needs_restart": ["canvas.width is written to the config file and takes effect when the mixer restarts"],
  "failed": [],
  "page": {}
}
```

`part` is `setting`, `source`, `output`, `channel`, `scene` or `media`.
`action` is one of:

| Action | Meaning |
|---|---|
| `add` | it is new here and will be added |
| `replace` | it is here already and the file's version takes its place |
| `remove` | it is here and not in the file, in replace mode |
| `keep` | it is here and stays as it is |
| `rename` | its id or name is taken; it arrives as `to` |
| `set` | a setting will be written; `note` says from what to what |
| `wait` | it cannot start as the file has it, usually for want of a key |
| `missing` | a clip the file names that is not in this mixer's media folder |
| `skip` | a different clip of the same name is here already, and it stays |

`waiting` is what a person still has to do afterwards. `needs_restart` names
each setting the import wrote that only takes effect on the next start.
`failed` is what was tried and refused, with the reason. `page` is the
file's page part, for the page to put back.

## What the file holds

A JSON object:

| Field | What it is |
|---|---|
| `format` | always `"godwinmix.project"` |
| `version` | the project format, 1 today |
| `written_by` | `"godwinmix 0.2.0"` |
| `exported_at` | milliseconds since 1970 |
| `name` | what the person called it |
| `secrets` | true when keys are inside |
| `settings` | show settings the config file sets, by dotted key: `canvas`, `program`, `multiview`, `snapshot`, `safety`, `stall` and `browser.overlay_fps` |
| `machine` | the rest of the config file's settings: addresses, folders, hardware, plugins, and the control token when `secrets` is true |
| `sources` | each source as the mixer runs it |
| `outputs` | each output, with its `rendition` when it asked for one |
| `channels` | each channel's record, and `secrets` with its keys and destination addresses when asked |
| `scenes` | the scene collection, as `scene.export` with `format: "json"` answers it |
| `page` | what the caller sent as `page` |
| `media` | `{name, size}` per clip, with `data` when `include_media` was set |
| `removed` | in words, what was taken out: `"output youtube: its stream key"` |

Only settings the config file writes itself travel. A default is not a
choice anybody made, and it is the default on the other machine too.

Without `include_secrets`, an RTMP address loses the part after the last
`/` when it is six characters or more (the stream key), an address loses a
password and any `key`, `psk`, `token`, `passphrase`, `password`, `secret`,
`streamid` or `auth` in its query, and parameters with those names are
dropped. Each entry that lost something says so in `$removed`.

JSON rather than a zip: the page reads and writes it with no library, it
travels over the protocol as itself, and a person can open it and see what
is in it before they send it anywhere.

## Refusals

Every refusal is `-32602` with `data.field: "file"` and a `data.reason`:

| `reason` | When |
|---|---|
| `not_json` | the text sent is not JSON |
| `not_object` | the JSON is not an object |
| `wrong_format` | there is no `"format": "godwinmix.project"` |
| `scene_collection` | it is a scene collection; open it with the Scenes panel's import or `scene.import` |
| `no_version` | there is no `version` |
| `newer_version` | a newer GodwinMix wrote it; `data` has `version`, `supported` and `written_by` |
| `damaged` | a part does not read; `data` names the source, output or channel |

A setting this mixer would refuse (a canvas width it cannot take, say) is
refused the way `config.set` refuses it, before anything else moves.

## What it does not carry

Filters added at run time, plugin settings and the `[[tokens]]` table stay
with the machine. The scene collection's asset files are not copied; a
scene that uses an image names it, and the Scenes panel's Fix says what is
missing.
