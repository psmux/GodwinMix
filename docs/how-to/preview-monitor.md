# Use the mixer controls

## Preview and programme

Two monitors sit across the top of the page. Preview is on the left and shows
what you have lined up. Programme is on the right, in a red frame, and shows
what the audience is seeing. Both are always there.

Click a scene tab and that scene comes up in preview. Nothing goes out: the tab
turns amber, the preview pane turns amber with it, and the programme carries on.
When you are happy with it, the bar under the monitors puts it on air. Cut
switches on the next frame. Auto fades, over the number of milliseconds in the
box beside it; three hundred is the default and the box remembers what you set
on this device.

After a take the two monitors change places, the way they do on a desk: what was
on air drops back into preview, so putting it back is one press. With nothing
armed the preview says "Nothing armed. Click a scene." and Cut and Auto are
greyed out.

The number keys still cut straight to air: 1 to 9 take the scene in that
position, 0 cuts to black. They are for the operator who knows the running
order and does not want two presses.

If you are working alone with three cameras and the two monitor flow is one
press too many, turn on **Tap cuts directly** in the Scenes heading. A tab tap
then goes straight to air, as it did before. It is off by default, and it is
remembered on this device only.

The monitor area has its own fixed place. Only the controls below it scroll. Sources, Scenes, Outputs and Media have collapsible headings. Their open state is remembered in this browser.

Sources are shared inputs. A scene arranges those inputs on screen, and the same source can be used in several scenes. A new empty collection starts with a Default scene, containing the configured sources when available. Drag sources onto a scene to add them, or press the small + on that scene's tab to add a new source straight into it, then double click the scene to edit its layout. Creating a scene does not put it on air.

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
