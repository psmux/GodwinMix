# Preview monitor status

The programme panel shows "Preview live" while it receives mosaic frames. After two seconds without a frame it shows "Waiting for preview frames". When the panel releases its subscription because it is hidden, it shows "Preview paused while hidden".

This status describes the preview connection. It does not confirm that an external output is receiving video. Each output has its own status in the footer.

Nonseekable source tiles show "Continuous live source". Seekable sources retain their position slider. These labels do not change playback.

## Panel placement

The `monitor` slot holds the programme panel. Saved layouts that put it in `main` are migrated automatically. The built in control panels use collapsible sections in `main`; outputs and media previously placed in the footer move there as well.

The source audio and seek calls send the required `id` field. Source and output management controls follow the same id contract. `program.take` continues to use `source` or `scene`.
