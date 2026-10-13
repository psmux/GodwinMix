# Install a plugin from the gallery

The Plugins page works like the app store on a phone. You find a plugin, press
Install, and when the button says Open it is ready. Nothing restarts and the
programme keeps going the whole time.

This page takes two minutes.

## Open the page

**Sources > More plugins…**, or **More plugins** at the bottom of the add
source picker. On a phone it is in the same menu.

The page opens on two rows. **Featured** is what the plugin list's
maintainers picked, drawn large. **New and updated** is what changed most
recently. Swipe either row sideways, or scroll it with a trackpad. Under them
is every plugin, as a grid. Searching or pressing a chip hides the two rows and
narrows the grid.

Hold the mouse over a card and it plays the plugin's clip, if it has one.

## Look before you install

Press a card's picture or name. The detail view opens on a large carousel of
the plugin's pictures and clips:

* the arrows, the dots underneath, a swipe, or the arrow keys once the
  carousel has focus move between them
* a GIF or a video plays only while its slide is on screen, muted and looping
* if your computer is set to reduce motion, nothing plays until you press
  **Play** on it

Below the carousel are the description, who makes it, the licence, the
version, whether it was signed, and a link to its source code.

## Install it

Press **Install**, on the card or in the detail view. The button shows what is
happening:

| The button says | What the mixer is doing |
|---|---|
| `Downloading 42%` | fetching the release from GitHub. The percent is there when GitHub says how big the file is |
| `Installing` | unpacking it, and for a Python plugin making its own environment with its dependencies |
| `Starting` | starting what the plugin runs on its own: a service, a device finder |
| **Open** | done |

All of this happens in the mixer, away from the programme. A slow download
does not slow the output, and you can keep working while it runs, or close the
page.

When it is done the plugin is ready with no other step:

* a plugin with a panel has its panel open in the workspace already. **Open**
  closes the Plugins page and brings that panel forward
* a source plugin is in the add source picker. **Open** opens the picker on its
  form
* a service is running. `gmx plugin list` shows it with a process id

## When it does not install

The card says what happened in one sentence, in red, and the next step under
it. The button turns into **Try again**. Common ones:

| The card says | Do this |
|---|---|
| `It did not install: <name> has no asset for windows-x86_64.` | The author has not built it for this computer. The next step names the platforms it has. |
| `It did not install: the releases of <owner>/<repo> could not be reached.` | The mixer has no route to GitHub. Check its network and press **Try again**. |
| `It did not install: python is not installed, or not on PATH.` | A Python plugin needs Python on the mixer's computer. Install Python 3 and press **Try again**. |

A card that says `Not for Windows`, `Not for this computer` or `Needs a newer
GodwinMix` before you press anything cannot run here, and its button says so
instead of Install.

## Update and remove

An installed plugin with a newer version says **Update**. The one you have
keeps working until the new one starts, and if the new one does not start the
old one stays.

**Remove** is in the detail view. It asks first, then stops the plugin and
takes away everything it added.

## Python plugins

A pure Python plugin's release holds its source, not a program. The mixer
makes a separate environment for it inside the plugin's folder: with `uv` when
`uv` is installed, otherwise with Python's own `venv`. On Windows that is
`python` when a real one is installed and the `py -3` launcher otherwise, never
the Microsoft Store stand in. Set `GMX_PYTHON` to use a particular Python.

An author can ship one asset for every computer, named with `any` where the
platform would go: `myplugin-1.0.0-any.zip`. Per platform names
(`myplugin-1.0.0-windows-x86_64.zip`) work too, and a mixer older than 0.3.10
understands only those.

## See also

* [plugin.gallery](../reference/plugin-gallery.md), what the page reads
* [The index format](../reference/index-format.md), for `media`, `preview`,
  `featured` and the `any` platform
* [Tasks](../reference/tasks.md), for the install's `stage` and `progress`
* [Install a plugin](install-a-plugin.md), for the terminal and every other
  source form
