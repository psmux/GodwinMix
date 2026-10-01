# Watch many shows at once

The monitoring wall is one screen with every show on the station: a row each,
or a tile each, with its picture, its input, where it goes and anything that
is wrong with it. It is built for a headend with a few hundred feeds as much
as for a church with three rooms. Nothing on it costs the station anything
until you open it.

## Open it

Any of these:

* View, Monitoring wall.
* The palette (`Ctrl+K`), then type "wall".
* The grid button at the end of the show tabs, after the `+`.

It opens over the page, under the top bar, so the menu and the show tabs stay
in reach. `Escape` or the `×` closes it. Opening it again comes back sorted and
laid out the way you left it; that is kept in this browser only.

## Read a row

From left to right:

* The picture, a small frame from the show taken every two seconds. A show
  gone black says so on the frame.
* The name, with a dot for its health (green fine, amber a warning, red an
  alarm, grey off) and under it whether it is running and which program of
  the feed it takes.
* The input: how it arrives (Multicast, SRT, RTMP, HLS and so on), its size,
  frame rate and codec, and its sound.
* The bitrate, as a number and a line of the last forty seconds, with lost
  packets and continuity errors under it in red when there are any.
* The outputs, one small chip each: a dot for its state, where it goes, and
  whether it is a copy of the input or a new format, with its own bitrate.
* Load: what this show's encodes cost the machine. A show that only copies
  says copy only.
* Alarms, each with how long it has been going: black, frozen, silent,
  stalled, no input, loss, CC errors, an output that failed, or a format the
  governor refused.
* Mixing: the switch that makes a show mixed (scenes, transitions and a
  programme encode of its own) or direct (the input straight to the outputs).

The line at the top counts every show on the station, not only the ones on
screen: how many, how many are live, how many are in alarm, the total in and
out, and the CPU and GPU the governor sees.

On a phone a row is the picture, the name, the bitrate and the alarms. The
tiles show more of each show.

## Rows or tiles

Rows fits forty shows on a laptop screen. Tiles shows bigger pictures, six
across at 1440 pixels and two on a phone. Switch with the Rows and Tiles
buttons.

## Find the shows that need you

* Sort by any column with a click on its heading; a second click turns the
  order round. The wall starts sorted by alarms, so a show in trouble is at
  the top.
* Type in the filter box. It looks through names, input and output
  addresses, transports and alarm words, and keeps the shows that match every
  word: `multicast black` is every multicast feed that has gone black.
* The second box narrows to the shows in alarm or warning, or to one kind of
  alarm.
* Group by state puts the shows in alarm first, then warnings, then the ones
  running, then the ones that are off, each under a heading with its count.

## Acknowledge an alarm

A new alarm flashes. Move to its row with the arrow keys and press `A`: it
stops flashing and stays listed with its age until it clears. If the same
alarm comes back later, it flashes again.

## The keys

| Key | What it does |
|---|---|
| `↑` `↓` | move from show to show (in tiles, `←` `→` as well) |
| `Home`, `End` | the first or the last show |
| `Page Up`, `Page Down` | ten shows at a time |
| `Enter` | open the show |
| `A` | acknowledge the show's alarms |
| `Escape` | clear the filter, then close the wall |

## Open a show

Click a row, or press `Enter` on it.

A mixed show opens in the mixer, the same as clicking its tab.

A direct show opens its detail, over the wall:

* Its input address, the program to take when the feed carries several, and
  a backup input the show switches to when the main one stalls and leaves
  when it comes back. Save input sends them.
* Its outputs. Format changes what one is sent as, with the same choices as
  Add destination: Same as the source costs nothing, a preset or Custom is
  encoded and admitted by the governor first. Turn off stops one without
  forgetting it. Add output takes an address (`udp://`, `srt://`, `rtmp://`)
  and a format.
* Its alarms: how long a black or frozen picture, or sound below a level,
  lasts before it counts. Untick Watch the picture and sound and the show
  stops decoding for them; no input, stall, loss and output alarms still work,
  because they need no decode.
* The mixing switch, with a sentence on what it does. Turning it on gives the
  show its own mixer with the input as its one source; turning it off is
  refused, with the reason, while the show uses scenes.

## What it costs

Opening the wall asks the station for the numbers of the shows on screen once
a second, in one call, and for their pictures every two seconds. Scroll and
the shows that left the screen stop being asked for. Two hundred shows on the
station cost the same as the forty you can see. The page draws only the rows
on screen and a few either side, so scrolling stays smooth.

A picture costs the station one keyframe decode for that show, about once a
second, while someone is looking. With the wall closed nothing is decoded
for pictures. A direct show still decodes keyframes and a little sound for
its black, freeze and silence alarms when they are switched on, which they
are by default.

A silence alarm on the show called `main` on a fresh station is real: that
is the station's own show, which composites, and with no source in it its
programme is digital silence. Its detail reads `Programme peak -350 dBFS`.

## From a script

The wall uses `show.list`, `show.stats {ids}`, `governor.status`, `show.set`
and `show.output.add`, `.set` and `.remove`, and listens for
`event/show.health`. A script or an agent watching a headend can do the same:
see [run several shows](run-several-shows.md) and
[add shows in bulk](add-shows-in-bulk.md).
