# Run the mixer from a phone or a tablet

The operator page is the same page on a phone, a tablet and a desktop. Open
the mixer's address in the phone's browser and it lays itself out for the
screen and the finger. Nothing has to be installed and nothing has to be
switched on. Several people can have it open at once, some on phones and
some at desks.

## Reach the mixer

The phone opens the same address a desktop browser does, `http://` then the
machine's address on your network and the control port, for example
`http://192.168.1.20:8080`. That only works when the control port listens on
the network rather than on `127.0.0.1` alone: `bind` under `[control]` in the
[configuration](../reference/configuration.md#control) decides that, and the
shipped default of `0.0.0.0:8080` already does. A mixer with a `token` asks
for it once and the browser keeps it.

## Find your way round a narrow screen

Below 760 pixels wide the page is one column. The menu bar becomes one ☰
button at the top, and every item in File, Edit, View, Sources, Scenes,
Outputs and Help is under it. The panels stack one under another and the page
scrolls between them; dragging a panel's title to rearrange the dock is a
desktop thing and is switched off here, but Panels and layout still opens and
closes panels.

Anything the keyboard does has a button or a menu item:

| On a keyboard | On a phone |
|---|---|
| `1` to `9` | tap the scene tab or the tile |
| `Space` (Studio mode) | the Take button on the take bar |
| `0` | Cut to black in the header, or ☰, Scenes, Cut to black |
| `Ctrl+K`, the palette | the ⌘K button in the header, or ☰, Edit, Command palette |
| `Ctrl+N` | the Add sources tile, or ☰, Sources, Add source |
| `Ctrl+F` | the Filter box over the tray |
| `F2`, `Delete`, `Ctrl+C`, `Ctrl+V`, `Ctrl+A` | a long press on the tile, then Rename, Remove, Copy, Paste or Select all |
| `Enter` | the ⚙ on the tile, or a double tap |
| `Ctrl+Z`, `Ctrl+Shift+Z` | ☰, Edit, Undo or Redo |
| `Ctrl+O`, `Ctrl+S`, `Ctrl+,` | ☰, File |
| `?` | ☰, Help, Keyboard shortcuts, which lists the touch gestures too |

## Use your finger

| Gesture | What it does | The mouse's way |
|---|---|---|
| tap | the same as a click: a tile goes on air, or is armed in Studio mode | click |
| long press, about half a second | the item menu: Put on air, Rename, colours, Settings, Copy, Remove | right click |
| double tap | open: a scene opens in the composer, a source's settings open, a show tab is renamed, a fader goes back to 0 dB | double click |
| drag by the ⠿ grip | move a tile, or drop a source on a scene | drag the tile |
| swipe anywhere else | scroll the page | the scroll wheel |

The grip is the six dots at the left of each tile's name bar. It is there so a
swipe across the picture still scrolls the tray, which on a phone matters more
than rearranging it. A tap on the grip that does not move selects the tile and
does not put it on air. A long press that turns into a menu never puts the
tile on air either, when the finger lifts. A button with no menu, Take for one,
still does its job when it is held down a moment too long.

A long press in a text box is left to the phone, for selecting and pasting.

Faders have a thumb sized for a finger. Slide sideways to change the level;
an up or down swipe that starts on a fader still scrolls the page.

## On a tablet

A tablet wider than 760 pixels gets the desktop layout, with targets sized
for a finger. The panel titles and the dividers between panes can be dragged
by finger to rearrange the dock, and a divider has a wider target than the
line you see. A touch laptop with a mouse as well keeps the mouse's sizes
and shows the ⠿ grip, so both work.

## What changes and what does not

The size of buttons, menu items, tabs and fields goes up to 44 pixels under a
finger, and text in fields is 16 pixels so iOS does not zoom the page when one
is focused. Controls that a mouse reveals by hovering, the ⚙ on a tile, the
level and mute strip, the empty routing cells, are always shown. The mixer
itself does not know or care what the page is on: the phone sends the same
calls the desktop does.

The keyboard reference has the same gestures beside the keys:
[the keyboard](../reference/keyboard.md#touch).
