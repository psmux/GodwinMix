# Use the mixer controls

The programme monitor starts above the scene and source controls. Drag a panel
heading to dock it beside another panel or group them as tabs. Drag the dividers
to resize. **Panels and layout** reopens panels and resets the workspace. The
arrangement is remembered in this browser. See
[Customize the workspace](customize-the-workspace.md).

Choose **Studio mode** in Programme to prepare a scene in Preview before taking
it live. **Cut** takes immediately; **Fade** uses the duration selected beside
it. Preview appears to the left of Programme on a wide panel and above it when
the panel is narrow. These controls use the same public commands as the CLI.
Closing or hiding the monitor releases its preview subscriptions.

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
