# Use the mixer controls

The programme monitor has its own fixed area. Only the controls below it scroll. Sources, Scenes, Outputs and Media have collapsible headings. Their open state is remembered in this browser.

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
