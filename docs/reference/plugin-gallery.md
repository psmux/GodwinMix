# plugin.gallery

Every plugin the mixer's marketplaces list, once each, with what the Plugins
page needs to draw a card and the detail view. Scope `read`.

    POST /api/v1/plugins/gallery
    {"jsonrpc": "2.0", "id": 1, "method": "plugin.gallery", "params": {}}

The handler is `crates/godwinmix/src/control/methods/plugin_gallery/`; the
fetching and the platform check are in
`crates/godwinmix-host/src/marketplace/gallery.rs`.

## Params

| Field | Type | What it does |
|---|---|---|
| `refresh` | boolean, default false | Fetch every marketplace now. Without it, a copy younger than ten minutes is read as it is. |

## What it does before it answers

1. If the community index (`psmux/godwinmix-plugins`) has never been added and
   was never removed, it is added. `GMX_DEFAULT_MARKETPLACE` names another
   default, or none when it is set to an empty string. `[marketplaces] only`
   wins: a pinned list that does not name the default never fetches it.
2. Each marketplace whose copy is older than ten minutes, or every one with
   `refresh`, is fetched again, giving each request four seconds.
3. One that cannot be fetched keeps the copy it had, carries a `problem`, and
   is left alone for a minute so a mixer with no network does not wait on
   every open.
4. Every cached copy is read and each plugin name becomes one entry, taken
   from the highest tier and then from the marketplace added first, which is
   the listing `plugin.add <name>` resolves to.

Nothing here runs unless the method is called. `plugin.search` and a
`plugin.add` of a bare name add the default the same way, but never fetch a
copy that is already on disk.

## The answer

| Field | Type | What it is |
|---|---|---|
| `platform` | string | This machine: `windows-x86_64`. |
| `offline` | boolean | A marketplace was due and could not be fetched, so what is shown is the copy from before. |
| `marketplaces` | array | `name`, `title`, `source`, `fetched` (seconds since the epoch, 0 for none) and `problem` when there is one. |
| `plugins` | array | One entry per plugin, highest tier first, then most stars. |

### An entry

| Field | Type | What it is |
|---|---|---|
| `name` | string | The plugin's name. |
| `title` | string | The listing's `title`, else the name. |
| `description` | string | |
| `tier` | string | `custom`, `bronze`, `silver` or `gold`. |
| `kinds` | array | What it provides. |
| `license`, `repository`, `homepage`, `author` | string | `author` falls back to the marketplace's `owner`. |
| `icon`, `thumbnail` | string | Picture URLs, for the browser to load. |
| `screenshots` | array | Picture URLs, GIFs and WebPs included. |
| `media` | array | Every picture and clip in order, each `{"url", "type"}` with type `image`, `animation` or `video`. Made from `screenshots` by extension when the index has none. |
| `preview` | string | The first animation or video, which a card plays on hover. Left out when there is none. |
| `featured` | boolean | The index put it in the Featured row. |
| `stars`, `downloads` | integer | GitHub stars, and every download of every release asset. Left out when the listing does not say. |
| `updated` | string | RFC 3339. |
| `tags` | array | |
| `bundled` | boolean | It ships with GodwinMix and installs with no download. |
| `version` | string | The newest listed version this mixer can run, else the newest listed. |
| `versions` | array | Every listed version, oldest first. |
| `signed` | boolean | That version's assets were signed by the marketplace's CI. |
| `install` | string | What to pass to `plugin.add` as `source`. |
| `marketplace` | string | The marketplace this entry came from. |
| `installed` | boolean | |
| `installed_version` | string | Present when installed. |
| `update` | boolean | Installed, and `version` is newer. |
| `runs_here` | boolean | It runs on this machine at this mixer's protocol level. |
| `fit` | string | The card's word for that: `Runs here`, `Not for Windows`, `Not for this computer`, `Needs a newer GodwinMix`, `Made for an older GodwinMix`, or `Not in this copy` for a bundled plugin this copy of the mixer was built without. |
| `fit_detail` | string | The sentence behind it. |

## Installing from it

`plugin.add {"source": <install>}`, then `task.get` on the task it answers
with, whose `stage` and `progress` say how far it is (see
[tasks](tasks.md)). When it completes, the plugin's services are running and
its panels are in `/plugins/index.json`; nothing restarts. `plugin.update
{"id": <name>}` and `plugin.remove {"id": <name>}` work on an installed entry.
The Plugins page does exactly this, then reads `/plugins/index.json` again and
opens the first panel the plugin brought.

## Pictures

The page loads `icon`, `thumbnail` and `screenshots` straight from where they
are. The page's content security policy allows pictures from
`raw.githubusercontent.com`, `opengraph.githubassets.com`,
`repository-images.githubusercontent.com` and `avatars.githubusercontent.com`
and nowhere else; a picture from another host shows as the kind's icon. A
video plays from `raw.githubusercontent.com` only.

Nothing moves unless somebody is looking. A card fetches its `preview` only
while a mouse is over it. In the carousel a GIF gets its address only while
its slide is on screen, a video is muted, loops, loads nothing until its slide
shows and pauses when it leaves, and with `prefers-reduced-motion` nothing
plays until its Play button is pressed.

## See also

* [Install a plugin from the gallery](../how-to/install-from-the-gallery.md)
* [Install a plugin](../how-to/install-a-plugin.md#from-the-plugins-page)
* [The index format](index-format.md), for the fields a listing carries
* [Publish a plugin](../how-to/publish-a-plugin.md#3-get-it-listed)
