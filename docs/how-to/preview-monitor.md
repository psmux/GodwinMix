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

Press **Studio mode** under the Programme monitor to get a Preview beside it.
You set up the next shot in Preview, then send it to Programme when you are
ready. Nothing reaches the audience until you do.

* Preview is on the left with a green frame, Programme on the right with a red
  one. On a narrow panel Preview sits above Programme.
* Click a scene tab, a scene tile or a source tile and it goes into Preview.
  The number keys do the same for scenes 1 to 9. Nothing goes on air.
* **Take**, the big button between the monitors, sends Preview to Programme
  with the transition chosen under it: any of the built in ones, with its
  direction (Wipe, Slide, Push) or colour (Dip), an easing and a length from
  0.25 s to 2 s, plus whatever the scene collection or a plugin adds. A small
  drawing beside the list shows the one chosen, and the choice is remembered in
  this browser.
  Space does the same, and so does a double click on the Preview picture.
* **Cut** sends it at once, with no transition.
* **Take the preview to programme** and **Cut the preview to programme** are in
  the palette (Ctrl+K) as well.

After a take, Preview keeps what you just sent, so both monitors show the same
shot until you pick the next one. That is what the mixer does with an armed
scene, and a source behaves the same way.

A scene whose sources are not all running cannot be taken. The Preview says
which ones are missing, and the picture shows the scene without them. Get them
running, or open the scene with the pencil beside its tab and delete them.

**Cut to black** at the top of the page, and `0`, go to black even while
something is in Preview.

Closing or hiding the monitor releases its preview subscriptions.

## Audio desk

Open **Audio** from **Panels and layout** for programme meters and a level and
mute control for each audio source. The same source controls in Sources and
Audio share their gesture state. Moving between panels does not change the
mix. Double click a fader to return it to unity gain.

The Audio panel stops painting its meters when hidden. It does not add a
second mix or a second media pipeline. These controls call `source.audio.set`
through the public client, and use the shared meter and fader modules that
plugin panels can import.

Resizing the preview stops its live backdrop before removing the compositor.
The replacement branch starts its consumers before its producers, so repeated
size changes do not send a live source into a missing downstream branch.
