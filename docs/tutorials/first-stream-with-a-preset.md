# Your first stream with a preset

This is the church path, timed: from a machine with nothing on it to a picture
going out to YouTube, in about ten minutes, most of which is YouTube's.

You need a machine with GodwinMix on it and a YouTube account. No cameras yet.
The preset starts with test patterns on purpose, so you find out whether the
stream works before you find out whether the cameras do.

Everything here happens on the page. You will not open a file.

## 1. Open the mixer (30 seconds)

Start GodwinMix and open the address it prints, which on that machine is
`http://localhost:8080`. The desktop app opens it for you.

Because nothing has been set up, the page asks what you are streaming and shows
five tiles. Press **Church service**.

It takes a second or two: it writes the configuration, the four scenes and the
layout, sets the theme, and brings up the sources and destinations that can
start without a restart.

## 2. Finish the checklist (3 minutes, mostly YouTube's)

The page now shows what is left, one row each, with a count at the top that
starts at "0 of 3 done".

**YouTube: needs a stream key.** Open YouTube Studio, then Create, then Go
Live. Pick "Streaming software" if it asks. The page shows a **Stream URL** and
a **Stream key**. Copy the key, paste it into the box on the row, and press
**Save**.

The row goes green and says the destination reconnects on its own. The mixer
never hands a key back to any client, so the count moving to "1 of 3 done" is
the mixer's own answer that the placeholder is gone, not this page assuming so.

**Facebook: needs a stream key.** The same, from the Live producer page. Leave
it if the service only goes to YouTube: a destination left on a placeholder
keeps trying and failing, which is noise in the log rather than a fault, and
the Outputs panel removes it.

**The camera plugin is not installed.** Press **Install camera support**. It
takes up to a minute and nothing restarts. The two cameras in this preset are
test patterns until it is there, which is the point: the stream is real before
the cameras are.

Then press **Go to the mixer**.

## 3. Put something on air (10 seconds)

Press the **Wide** tile.

The programme monitor at the top shows the test pattern, and the tile takes a
red frame, which on this page always means "this is going out".

The tiles are icons rather than moving pictures, because that is what the
church preset chose: a machine at the back of a hall keeps its cores for the
encoder. Settings changes it.

Go back to YouTube Studio. Within about twenty seconds the preview fills in and
the health indicator turns green. That is your stream.

## 4. Check it is healthy

The Outputs panel along the bottom has a row per destination, with its state
and its reconnect count. `live` with no reconnects is what you want.

A reconnect count that climbs means the upload is not keeping up. A lower
programme bitrate is the fix; 3000 looks better steady than 4500 stuttering.

The Alerts panel carries anything the mixer wants to tell you, in the words the
far end used.

## What you have

A real stream, with the wrong pictures in it. That is the point: everything
between the camera and YouTube is now known to work, so the next thing you
change is the only thing that can break.

## Next

**Real cameras.** Once the camera plugin is installed, press **+ Add source**
in the Sources panel and pick your camera off the list. The picker names the
cameras this machine can see, so there is nothing to type. Then remove the test
pattern: a source keeps the address it was made with, so a real camera is a new
source rather than an edit.

**The lyrics.** The `lyrics` source points at a page on this machine. Add one
pointing at your presentation software's address instead, and give that page a
transparent background: if it is white in a browser it is white on air.

**The slides.** Drag a video file onto the page. It lands in the media library
and can be added as a source from there.

**Stopping.** Press the tile again to go to black, stop each destination in the
Outputs panel, then stop YouTube's end.

## If it does not work

**Nothing at `localhost:8080`.** The mixer printed an address when it started;
use that one. On a server, this preset listens on every address, so use the
server's own, and set a token before it is on a network anybody else is on.

**The page asks for a token.** Whoever started the mixer has it. It is saved on
that device, so you are asked once.

**A destination says `reconnecting`.** The key is wrong, or YouTube has not
started its side. The button on that row says **Add key** while the mixer still
reads the address as a placeholder and **Edit** afterwards; either one takes a
new key. The Alerts panel carries the reason the server gave, verbatim.

**A tile is black.** The Sources panel says what each one is doing. A source
that says `connecting` has nothing arriving at that address yet.

**Everything else.** Ctrl+K opens the command palette, which lists every method
this mixer publishes, `core.doctor` among them: it checks the elements, the
encoders, the ports, the configuration and the disk, and its answer is the
first thing to paste into an issue.
