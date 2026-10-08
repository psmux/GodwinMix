# Use the mixer controls

The programme monitor starts above the scene and source controls. Drag a panel
heading to dock it beside another panel or group them as tabs. Drag the dividers
to resize. **Panels and layout** reopens panels and resets the workspace. The
arrangement is remembered in this browser. See
[Customize the workspace](customize-the-workspace.md).

Sources are shared inputs. A scene arranges those inputs on screen, and the same source can be used in several scenes. A new empty collection starts with a Default scene, containing the configured sources when available. Drag sources onto a scene to add them, then double click the scene to edit its layout. Creating a scene does not put it on air.

Outputs are destinations for the programme feed. They are separate from scenes, so changing a scene does not restart the stream or recording. Media is the shared file library.

For a manually operated mixer that should switch immediately, set this in the mixer configuration and restart it:

```toml
[safety]
min_hold_ms = 0
flash_guard = false
max_takes_per_minute = 120
```

A configured shot hold still applies to sources, scenes and media takes. Automation installations may choose a longer hold. Set `flash_guard = true` to retain the separate brightness based cut hold.

The audio slider changes volume. Mute is independent of volume, and unmuting restores that level. Seekable clips have a position slider. Test generators, cameras and live streams show "Continuous live source" because they have no end position or loop setting.

## Studio mode

The page opens in Studio mode: a Preview beside the Programme monitor, and Take
between them. You set up the next shot in Preview, then send it to Programme
when you are ready. Nothing reaches the audience until you do. Press **Studio
mode** under the monitors to turn it off; the page remembers that in this
browser and opens the same way next time.

* Preview is on the left with a green frame, Programme on the right with a red
  one. On a narrow panel Preview sits above Programme, with Take and Cut in a
  band between them.
* Click a scene tab, a scene tile or a source tile and it goes into Preview.
  The number keys do the same for scenes 1 to 9. Nothing goes on air.
* **Take**, the big button at the top of the bar, sends Preview to Programme
  with the transition shown under it. Space does the same, and so does a
  double click on the Preview picture.
* **Cut**, under Take, sends it at once with no transition.
* Under them is the transition Take uses, as a drawing, a name and a length
  ("Wipe left, 0.5 s"). Press it to open the list of every transition, each one
  moving: the built in ones, whatever the scene collection or a plugin adds,
  and the ones imported from packs. The direction (Wipe, Slide, Push) or
  colour (Dip), the easing and the length from 0.25 s to 2 s are at the top of
  that list. Escape, Done or a press outside it closes it, and nothing in it
  moves while it is closed.
* The three small buttons under that are quick picks: the three transitions
  this browser has taken with most, Fade, Wipe and Dip until it has taken
  with any. One press makes it the next take's.
* **Effects** opens the effects from packs, each with its key. They play over
  whatever is on air; Alt+1 to Alt+9 play the first nine without opening it.
  A mixer with no effects has no Effects button.
* **Take the preview to programme** and **Cut the preview to programme** are in
  the palette (Ctrl+K) as well.

Take and Cut stay where they are at every width. The bar never grows with the
fx library: a hundred imported transitions are a hundred tiles in the list,
not a hundred buttons beside Take.

### What Preview shows when nothing is armed

Preview is never just black. With nothing armed, or with the armed scene
already on air, it shows the scene most likely to be taken next, and says so
beside its name:

* **Suggested: on air before this** is the scene that was on air before the
  one on air now. After a take, from this page or from anyone else (another
  operator, an AI agent, `gmx ctl take`), Preview swaps to where the show came
  from, the way a vision mixer swaps its buses.
* **Suggested: next scene** is the first scene that is not on air, when this
  page has not seen anything else on air yet.

Take and Space take the suggestion. It is this page's own guess: it is not
armed on the mixer, other pages do not see it, and the frame round Preview is
dashed while it shows one. Click a scene and that is what Preview holds.

A source in the previewed scene that has no picture yet shows its name and its
state on its own box ("Late camera, connecting, no picture yet") rather than a
black hole. The take still goes ahead without it, and the note above the
picture names it so it can be fixed first. With no scenes and nothing armed,
Preview says what to do instead.

**Cut to black** at the top of the page, and `0`, go to black even while
something is in Preview.

Closing or hiding the monitor releases its preview subscriptions.

## Input levels

Every tile in **Sources** that carries sound has a level bar between its
picture and its name. It is there without hovering and in every gallery mode,
so a glance at the tray says which inputs are making a sound. The bar is the
core's own measurement at the point the sound reaches the mix, after the
source's gain and before its mute, ten times a second.

A source with sound and no picture, a microphone or a line input, has no
picture box. Its tile says **Sound only, no video**, its bar is wider, and the
loudest channel is printed in dBFS beside it. A microphone that has not sent
anything yet says **Sound only, nothing heard yet**; if it stays like that, the
device is held by another program or the operating system has not given the
mixer permission to use it.

**Settings > Show meters on tiles** turned off takes the bars away. With no
meter on the page the page stops asking the core for levels, so they cost
nothing.

## Audio desk

Open **Audio** from **Panels and layout** for programme meters and a level and
mute control for each audio source. In the default layout it is a tab beside
Graphics. The same source controls in Sources and
Audio share their gesture state. Moving between panels does not change the
mix. Double click a fader to return it to unity gain.

The Audio panel stops painting its meters when hidden. It does not add a
second mix or a second media pipeline. These controls call `source.audio.set`
through the public client, and use the shared meter and fader modules that
plugin panels can import.

Resizing the preview stops its live backdrop before removing the compositor.
The replacement branch starts its consumers before its producers, so repeated
size changes do not send a live source into a missing downstream branch.
