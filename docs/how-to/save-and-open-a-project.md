# Save and open a project

A project file holds everything you set up on a mixer: its sources, its
destinations and the formats they send, its channels, its scenes, the
picture size and stream settings, and the way the panels are laid out. Save
one to keep a setup, to move it to another machine, or to hand it to the
next person who runs the show.

## Save the project

1. Open **File > Save project as** (or press `Ctrl+S`, `Cmd+S` on a Mac).
2. Give it a name.
3. Leave **Include stream keys and channel keys** off if the file is going to
   someone else. Turn it on for a backup of your own.
4. Leave **Include the clips themselves** off unless the media folder is
   small. Off, the file lists each clip by name and size.
5. Press **Save**. The browser downloads `<name>.gmxproject`. In the desktop
   app the system's own save dialog asks where it goes.

## Open a project

1. Open **File > Open project** (or press `Ctrl+O`) and choose the
   `.gmxproject` file. You can also drop the file anywhere on the page.
2. Choose what to do with what is here:
   * **Replace this mixer's setup**: the file's sources, destinations,
     channels and scenes take the place of this mixer's.
   * **Add it beside what is here**: nothing is removed, and anything whose
     name is already taken arrives with `-2` on the end.
3. Read the summary. It counts what would be added, replaced, removed and
   renamed, and lists anything left to do afterwards. **Every change** opens
   the full list. Nothing has changed yet.
4. Press **Open project**.

When the file was saved without keys, destinations wait for their stream
key and each channel gets a new key. The summary says which. Give the keys
again in Outputs and Channels.

Clips that were not in the file are listed with their size. Copy them into
the mixer's media folder (Mixer settings shows where it is), or save the
project again with the clips included.

Some settings, the picture size for one, only take effect when the mixer
restarts. The summary names them, and the bar at the top of the page offers
the restart.

## Start a new project

**File > New project** takes every source, destination, channel and scene
off this mixer, including anything on air, after asking. Save the current
project first if you want it back. The mixer's settings stay as they are.

## From a script

The page uses `project.export` and `project.import`, which any client with
an admin token can call. See [Project files and the project
methods](../reference/project.md).
