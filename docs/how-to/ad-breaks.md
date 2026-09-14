# Ad breaks


Clips live in a directory on the machine running the mixer, set by `media.dir`.
The UI lists them with their durations and marks any without an audio track;
click one to arm it, double-click to roll it straight away.

The library is server side on purpose. A browser file picker returns a file from
the operator's own machine with no usable path, and the mixer needs something it
can open itself, so a picker would only ever work when the operator happened to
be sitting at the server. Listing a directory works identically over the network.

Interrupt the programme with a file, then rejoin live:

```sh
curl -X POST -H 'Content-Type: application/json' \
  -d '{"uri": "/path/to/ad.mp4"}' http://localhost:8080/api/adbreak
```

There is no time shift buffer, by design. The source keeps running behind the ad
and the mixer rejoins it live, so whatever played during the break is not shown.

An ad is just another source as far as the mixer is concerned. It is decoded and
normalised to the same canvas contract as a camera, which is why it reuses the
same take, the same audio crossfade and the same slate behaviour, and why
switching to and from it costs the output nothing.

Five things had to be handled to make this work inside a live programme:

* **A shared clock.** Every source pipeline is put on the program pipeline's
  clock and base time. Live RTMP inputs get away without it because their timing
  comes from arrival, but a file's timestamps start at zero.
* **Rebasing.** The ad's pads are offset onto programme time when it rolls, with
  the same offset on video and audio so it stays in lip sync.
* **Ending on the clock, not on end-of-stream.** EOS arrives while more than a
  second of already-decoded ad is still in flight through the queues. Returning
  to live at that moment truncates the ad: an eight second file played for 6.6
  seconds. The return is scheduled from the file's own duration instead, and now
  plays all 240 frames.

* **Aligning every source's timeline.** Each input lives in its own pipeline, so
  its segment starts when that source starts and its buffers carry running times
  beginning near zero while the programme may be hours in. A compositor hides
  this by reusing the frame it holds, so video looked right; an audiomixer
  cannot place samples it has no valid position for and discarded every one.
  Cameras appeared perfectly live and carried **no sound at all**. Each source's
  mixer pads now take an offset, computed from its first segment and shared
  between video and audio so lip sync is preserved.
* **Declaring the mixers' upstream latency up front.** Attaching a branch to a
  running aggregator otherwise makes the whole pipeline recalculate its latency,
  and output pauses while it settles. Rolling an ad cost about a second of
  programme that way.

Pass `at_running_time_ms` to place the break on a specific frame. A scheduled
break only arms a timer: its pipeline is built shortly before the cue, because
one held paused for six seconds rolled to black for its whole duration.

