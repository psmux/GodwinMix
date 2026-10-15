# Show the programme on a projector or a second screen

Put the live programme on a projector, an LED wall or a monitor plugged into
this computer over HDMI, with nothing else on it: no menus, no labels, no
cursor. Streams and recordings carry on whatever you do with it.

There are two ways. Full screen fills the screen you are working on, which is
right for a quick look or a laptop that is the only screen. Programme on a
display opens a separate output on the projector and leaves your own screen
free to run the show.

## Full screen on this screen

On the Programme monitor, press the full screen button (⛶) in its title bar,
or press `F`, or double click the picture. View > Full screen programme does
the same.

The programme fills the screen at its own shape, with black bars where the
screen's shape differs (a 16:9 programme on a 16:10 laptop has thin bars top
and bottom). The cursor goes after two seconds without moving the mouse.

To leave, press `Esc` or double click. The programme, the streams and the
recordings are not touched; the picture on your screen is the only thing that
changes.

## Programme on a display

1. Plug the projector in and let the computer find it. In the system's
   display settings, set it to extend the desktop rather than mirror it.
2. Press **Display** on the Programme monitor, or choose View > Programme on a
   display.
3. Choose the projector from the list. Each display is listed by name and
   native resolution, for example `EPSON PJ, 1920 × 1080`. The one you are
   working on says `(main display)`.

The programme fills the projector. Your own window stays as it was: keep
working in it, move it, reload it. The output has its own connection to the
mixer, so none of that reaches the projector.

To stop the output, choose **Stop the programme output** in the same list,
or click the projector's picture and press `Esc`.

### In the desktop app

The list is the app's own menu and shows every display the computer has. The
output opens borderless and full screen on the one you chose.

The app remembers the display. If the output was open when you quit, it opens
again on that display the next time the app connects, as long as the display
is plugged in. If the display is unplugged while the output is open, the
output closes rather than jumping onto your own screen, and it comes back by
itself when the same display is plugged in again, in the same place in the
display settings.

### In a browser

Chrome and Edge can list the displays. The first time you press **Display**,
the browser asks whether the page may manage windows on all your displays;
answer **Allow**. The output opens on the display you chose. If the browser
does not fill the screen by itself, click the picture once, then press `F` or
double click it.

On the projector, the first `Esc` leaves full screen (the browser keeps that
key for itself) and the second closes the output.

Firefox and Safari cannot list displays. **Display** then offers **Open the
output in a new window**: drag that window onto the projector, then double
click it to fill the screen.

The page remembers the display you chose last and lists it first.

## Picture quality

The output shows the programme itself, not the small Programme monitor:

* the programme's own resolution, so a 1080p programme is 1920 × 1080 on a
  1080p projector, pixel for pixel. A smaller display gets a picture scaled
  to its width by the mixer; a larger one gets the programme's own size,
  scaled up by the display;
* the programme's own frame rate, 30 fps for a 30 fps canvas, whatever the
  Programme monitor's frame rate menu says;
* about one frame behind the programme, plus the network: each frame is sent
  on its own, with nothing buffered ahead of it.

The mixer makes this picture only while a full screen view or an output is
open, and stops when the last one closes. It costs the mixer one JPEG encode
of the programme per frame, about half a CPU core for 1080p at 30 fps on a
laptop with no GPU. It can never slow the stream or the recording: a mixer
that cannot keep up drops frames from the projector, never from the
programme.

Over a network the picture is around 5 to 9 MB/s at 1080p30. On the same
computer, or on a wired network, that is nothing; over Wi-Fi to a second
machine the frame rate falls before the picture freezes.

## When something is wrong

* **The list has only one display.** The projector is mirroring the main
  screen, or is not detected. Set it to extend the desktop in the system's
  display settings, then press **Display** again.
* **The browser says it was not allowed to list the displays.** Allow
  window management for the mixer's address in the browser's site settings,
  or use **Open the output in a new window**.
* **The browser blocked the output window.** Allow pop ups for the mixer's
  address.
* **The picture on the projector is black.** Check that the Programme monitor
  shows a picture. If it does, close the output and open it again.

See also [the streams reference](../reference/streams.md#extprogramme) for the
protocol behind this, and
[why the output is sent to a browser](../explanation/full-screen-output.md).
