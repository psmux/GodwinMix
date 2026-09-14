---
name: clock
description: Put the time on screen. Use this when a show needs a visible clock, a countdown to the top of the hour, or a burned in timecode for a recording. Example, add the source and set its format: gmx source add clock --set format="%H:%M:%S".
---

# Clock

A source that draws the current time on a solid background.

## Settings

* `format` is a strftime string. `%H:%M:%S` is the default.
* `background` is a hex colour.
* `font_size` is a fraction of the canvas height, 0.1 to 0.8.

## Notes

This copy exists so the fixture's manifest points at a real file. The plugin it
describes is not in this directory and never was.
