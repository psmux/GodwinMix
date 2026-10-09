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

* The picture, a small frame from the show taken every two seconds, for a
  direct show and for one that mixes alike. A show gone black says so on the
  frame. A show that is stopped, failed or still starting says that where
  its picture would be, and is not asked for one.
* The name, with a dot for its health (green fine, amber a warning, red an
  alarm, grey off) and under it whether it is running and which program of
  the feed it takes.
* The input: how it arrives (Multicast, SRT, RTMP, HLS and so on), its size,
  frame rate and codec, and its sound.
* The bitrate, as a number and a line of the last forty seconds, with lost
  packets and continuity errors under it in red when there are any.
* The outputs, one small chip each: a dot for its state, where it goes, and
  whether it is a copy of the input or a new format, with its own bitrate.
* Load: what the show costs the machine, as a share of one core or a
  number of cores. For a show that mixes it is the show's own process,
  compositing and the programme encode together, and it says mixing until
  the first reading arrives. For a direct show it is the encodes of the
  outputs given a format. A direct show whose outputs all copy says copy
  only.
* Alarms, each with how long it has been going: black, frozen, silent,
  stalled, no input, loss, CC errors, an output that failed, or a format the
  governor refused.
* Mixing: the switch that makes a show mixed (scenes, transitions and a
  programme encode of its own) or direct (the input straight to the outputs).

The line at the top counts every show on the station, not only the ones on
screen: how many, how many are live, how many are in alarm, the total in and
out, and the CPU and GPU the governor sees. While a channel has a destination
failing it adds how many channels are in alarm, `1 channel in alarm`. The CPU counts the station's
own process, every show process and the ingest plugin, which runs every
direct show; it is read when the wall asks, every three seconds. On
Windows another process's CPU cannot be read, so there the figure is the
station's own process, what the governor has admitted, and what shows
holding a rendition report of themselves.

On a phone a row is the picture, the name, the bitrate and the alarms. The
tiles show more of each show.

## Channels on the wall

Under the shows, a Channels group lists every channel stream, each in a row
of its own: an encoder publishing to `sunday-service/main` and another to
`sunday-service/cam2` are two rows. A channel nothing is publishing to is one
row that says it is waiting for an encoder, and one that is switched off says
so.

A channel row has:

* The picture of what the encoder is sending, renewed every two seconds
  while the row is on screen and the stream is live. It feeds no source and
  needs none.
* The channel's name, with the health dot, and under it the stream's name,
  whether it is live or idle, and the mixer source it feeds when it feeds one.
* How it arrives (RTMP, RTMPS, SRT or WHIP), its size, frame rate and codec.
* Its bitrate, sound included.
* How many of the destinations sending this stream are sending, as
  `3 of 4 sending`. The chip turns red while a destination is retrying or has
  failed, and the alarms column names it with the reason, for example
  `Facebook: the far end closed the connection`. The dot and the row turn red
  with it.

The filter box looks through channel and stream names too, and `Any alarm`
or `Output failed` keeps the channel streams with a failing destination.
Click a channel row and the wall closes on the Channels tab, where its keys
and destinations are.

A mixer that keeps one show has a wall all the same: its channels are on it.

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

A picture of a direct show costs the station one keyframe decode for that
show, about once a second, while someone is looking. A picture of a show
that mixes costs that show's process one frame a second scaled down from its
programme, and a small JPEG each time the wall asks; it builds no mosaic.
Either way the work goes ten seconds after a show was last asked for, so
with the wall closed nothing is made for pictures. A direct show still
decodes keyframes and a little sound for its black, freeze and silence
alarms when they are switched on, which they are by default.

The Channels group reads `channel.list` once a second or two while the wall
is open and the tab is visible. A channel stream's picture costs the ingest
plugin one keyframe decode about once a second, while the row is on screen,
and stops ten seconds after the wall last asked. With the wall closed no
channel is decoded for a picture.

The show called `main` on a fresh station is the station's own show. It
composites and has no source, and it raises no alarm for that. A show that
composites judges black, freeze and silence on its programme only with its
alarms on (`enabled` in the show's alarm settings, or `[vitals] alarms =
true` in its config), and silence only while a source with sound is heard on
programme. The slate has nothing to fall quiet. Take a source with sound to
`main` and switch its alarms on, and a muted or quiet source raises
`silence` after `silence_secs`.

## From a script

The wall uses `show.list`, `show.stats {ids}`, `governor.status`, `show.set`
and `show.output.add`, `.set` and `.remove`, and listens for
`event/show.health`; for its channels, `channel.list` and
`GET /api/v1/channels/<id>/streams/<name>/thumbnail.jpg`. To be told when a
channel's destination fails rather than watching for it, use the
`channel.destination.state` [hook](hooks.md). A script or an agent watching a
headend can do the same:
see [run several shows](run-several-shows.md) and
[add shows in bulk](add-shows-in-bulk.md).
