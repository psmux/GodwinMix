# Sign in from a browser

The mixer asks for a token before it answers, unless it was set up with
none. The desktop app makes that token itself the first time it starts, so
you never chose it and may never have seen it. This page says where to find
it and how to open the mixer in a browser already signed in.

## On the computer that runs the mixer

In the mixer's page, choose **Help > Address and token**. The dialog shows:

* the address the page is on, with **Copy**
* the token this page signed in with, hidden until you press **Show**, with
  **Copy**
* **Open in a browser**, which opens the system's browser on the mixer,
  already signed in
* **Copy sign in link**, the same link to paste somewhere else

The sign in link is the address with `#token=...` after it. A browser never
sends the part after `#` to the server, and the page moves the token into its
own storage and off the address bar before it draws anything.

In the desktop app, **GodwinMix > Open in browser** (the tray icon has it
too) does the same as the dialog's button. The token itself is in a file
named `core-token` in the app's data folder (see
[The desktop app](desktop-app.md)).

## When a browser asks for the token

A browser that opens the address without the token asks for it. Paste the
token from **Help > Address and token** on the mixer's computer, or open the
mixer from that dialog in the first place. If you set the token yourself in
**Mixer settings** under **Control token**, it is that one.

## From a phone or another computer

Do not hand out this token. Whoever has it runs the mixer with the same
rights as your own page, and the only way to take it back is to change it,
which signs out everything that uses it. Use **Help > Open on another
device** instead: each device gets a token of its own as a QR code or a
link, and the list in that dialog takes any one of them back. See
[Run a show from phones](run-a-show-from-phones.md).
