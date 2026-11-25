# Use the teleprompter

GodwinMix has a teleprompter of its own. You write the script on the desk, a
phone, a tablet or the monitor under the glass opens `/prompter/`, and every
one of those screens scrolls the same words at the same place. Speed, pause
and the jumps between sections come from the desk, from the screen's own
keys, from a pedal, from a Stream Deck or from an agent. Nothing extra runs
on the mixer while it scrolls.

## Write a script

1. View > Prompter opens the Prompter panel.
2. Pick **A new script** in the list at the top (it is already picked when
   there are none), give it a title and type or paste the words.
3. Press **Save and put on the prompter**.

A few things in the text do something:

* A line that starts with `#` is a marker: `# Weather`. It shows on the
  screens as a yellow heading, and it is a place you can jump to.
* `*a word*` is drawn in italics and `**a word**` in bold.
* A blank line is a blank row.

Short lines read best. A long line is broken into rows at spaces, the same
way on every screen.

The scripts are kept on the station, beside the show's config, and come back
after a restart. The prompter comes back paused at the last place it stopped
or was sent to.

## Open a screen

Press **Open on a screen** in the panel. **Open here** opens the page in a new
window on this computer. The code is for a phone or a tablet on the same
network: scan it and the prompter opens full screen.

On a mixer that asks for a token, the code carries a read token made for that
screen, so the phone can follow the prompter and nothing more. The token
shows in Help > Open on another device, where you can take it back. Making
one needs an admin token on the desk; without one the dialog says so and the
screen asks for a token when it opens. You can also add a token to the
address yourself: `/prompter/#token=<token>`.

Any number of screens can be open. Each works out where the words should be
from the mixer's clock, so a phone and a monitor beside each other stay well
inside one row of each other.

## Drive it

From the panel: **Play** and **Pause**, **Slower** and **Faster** (a tenth of
a row a second each press), **Top**, **Back** and **Next** for the markers, a
button for each marker, and the slider to move anywhere in the script.

From the screen, a tap brings up the same buttons for a few seconds, and the
keyboard works:

| Key | What it does |
|---|---|
| Space or K | Play or pause |
| Down or Right arrow | Faster |
| Up or Left arrow | Slower, then backwards below zero |
| Page Down or `.` | The next marker |
| Page Up or `,` | Back to the start of this section, or the one before |
| Home and End | The top and the end |
| M | This screen's mirror (below) |
| F | Full screen |

Most foot pedals and presentation clickers send Page Down and Page Up, so
they move between markers with nothing to set up. A pedal that sends the
arrows changes the speed.

A screen opened with a read token can follow but not drive. Its keys say so
on the screen.

## Text size, margins, cue line and mirror

Under the transport in the panel:

* **Characters a row** is the text size. Every screen fits that many
  characters across its own width, so fewer means bigger words. The place in
  the script is kept when you change it.
* **Margin %** is the space left on each side.
* **Cue line** is where the line being read sits, as a fraction of the screen
  height from the top. The screens draw it as a band with an arrow.
* **Mirror for glass** flips the words left to right on every screen, for a
  monitor read through beam splitter glass.

A screen can go its own way on the mirror: press M or the mirror button on
the screen to cycle between as the desk, on here and off here. A phone on the
floor stays readable while the monitor under the glass is mirrored. The
choice is kept in that browser.

## From a Stream Deck or Companion

The Companion module has five prompter actions: play or pause, faster or
slower, next or previous marker, go to a marker, and load a script. See
[Put GodwinMix on a hardware panel](companion-and-streamdeck.md).

## From an agent

The tools are `save_prompter_script`, `load_prompter_script`,
`prompter_play`, `prompter_pause`, `prompter_speed`, `prompter_seek`,
`prompter_state` and `list_prompter_scripts`. They are found through
`search_tools`. From a shell:

```sh
gmx tool save_prompter_script '{"title": "Agent read", "text": "# Start\nThis came from an agent.\n# End\nThat is all."}'
gmx tool load_prompter_script '{"id": "agent-read", "play": true}'
gmx tool prompter_speed '{"by": 0.2}'
gmx tool prompter_seek '{"marker": "End"}'
```

The methods and their errors are in [the prompter reference](../reference/prompter.md).
Following the speaker's voice is meant for a plugin, which would send
`prompter.speed` as it listens; there is none yet.
