# Add shows in bulk

Two hundred channels off a headend are two hundred shows. You do not make
them one at a time: paste the list, fix what needs fixing, check what it will
cost, and add them in one go.

## Open it

From the monitoring wall, Add shows. Or from the menu, File, New shows.

## Give it the list

Paste into the box, or drop a CSV file on it, or use Choose a CSV file. Then
Read the list. Three shapes work:

One feed per line. Each line is an input address and each becomes a direct
show named after its address, with no outputs yet:

```
udp://@239.1.1.1:5000
udp://@239.1.1.2:5000
srt://feeds.example:9000/news
```

A bare multicast group and port, `239.1.1.1:5000`, is read as
`udp://@239.1.1.1:5000`.

CSV with these columns, in this order, or in any order with a header line
naming them:

```
name,input,program,output,format
News,udp://@239.1.1.1:5000,1,udp://10.0.0.50:6000,copy
Sport,udp://@239.1.1.2:5000,1,udp://10.0.0.50:6002,copy
Movies,udp://@239.1.1.3:5000,2,srt://cdn.example:7000,youtube-720p30
```

* `name` is what the show is called on the wall and on its tab.
* `input` is where it comes from: `udp://`, `srt://`, `rtmp://`, `rtsp://`,
  an `https://` HLS address, `rist://`, `file://`, or a channel stream as
  `channel:<channel>/<stream>`.
* `program` picks one program out of a feed that carries several. Leave it
  empty for the first.
* `output` is where it goes. Several outputs go in one cell, separated by
  `;`.
* `format` is `copy` (or empty) to send the input's own bytes, or a preset
  such as `youtube-720p30` to encode a new format. The box suggests the
  presets this machine has.

Cells copied out of a spreadsheet paste as tab separated lines, which work
the same way. Fill in an example puts three multicast feeds with UDP outputs
in the box, to start from.

## Fix it

The list becomes a table. Every cell is a box: click it and type. The `×` at
the end of a row takes it out. Nothing has been sent to the station yet.

## Check it

Check asks the station what it would do with the whole list, without doing
it. The answer appears under the table:

* How many shows are ready to add, and whether this machine has room for
  them.
* What they cost: CPU for the encodes, upload, memory. A list of copies costs
  no encodes at all.
* Any row the station refuses goes red, with the reason under it: a name
  already taken, an address it cannot read, a format this machine cannot make.

Fix a refused row and Check again, or leave it.

## Add them

The button says what it will do: Add 200 shows, or Add the 198 that are ready.
Every show that fits is added whole; a show is never half made. The wall
fills in as they start. Rows that were refused stay in the table with their
reasons, so you can fix them and add them after.

When the encodes do not all fit, the shows whose formats fit are added and
the rest are refused. Set some formats to `copy` and Check again to add them
all.

## From a script or an agent

The page sends one call, `show.add_many`, once with `dry_run: true` for Check
and once without it for Add:

```json
{"method": "show.add_many", "params": {"dry_run": true, "shows": [
  {"name": "News", "compositing": false,
   "input": {"uri": "udp://@239.1.1.1:5000", "program": 1},
   "outputs": [{"uri": "udp://10.0.0.50:6000", "rendition": null}]}
]}}
```

The answer is `{added, refused: [{index, name, why, data}], plan: {cost,
fits}}`. An agent can read the cost and decide before it applies.
