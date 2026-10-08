# Run a show from phones

The mixer runs on one computer. You want to take shots from a phone at the
back of the hall, or hand a tablet to someone running the scenes, while
somebody else keeps the desktop. Each phone opens the same page the desktop
shows, signed in with a token of its own that you can take back at any time.

It takes three steps: let the network reach the mixer, scan a code on each
phone, and accept the mixer's certificate once per phone.

## 1. Let other devices on the network reach the mixer

**In the desktop app**, open the GodwinMix menu (the first menu on macOS,
the app's menu bar on Windows and Linux) and turn on **Let other devices on
this network connect**. The app asks first, because the mixer restarts to do
it and the programme stops for a few seconds. Do it before the show.

From then on the mixer answers on the computer's network addresses as well
as on the computer itself, on a port it keeps across launches, so a link a
phone saved yesterday still works today. It is off by default. While it is
off the mixer answers on `127.0.0.1` only and nothing else on the network can
see it.

The port is never open: the mixer is always started with the app's own
token, and a phone needs a token of its own to get in. Turning the setting
off again shuts every phone out at once; their tokens are kept, so turning it
back on lets them in again.

**On a server you started yourself**, bind the control port to the network
and set a token, then restart the mixer:

```toml
[control]
bind = "0.0.0.0:8080"
```

```sh
GODWINMIX_TOKEN="$(openssl rand -hex 32)" godwinmix --config godwinmix.toml
```

A mixer with no token answers anyone who can reach it, as admin. Device
tokens are refused on such a mixer, because they would protect nothing; the
error says so and says how to set one.

## 2. Make a code for each phone

On the desktop, open **Help > Open on another device** (on a narrow window
it is under the ☰ menu, in Help).

1. Pick the address if there is more than one. The computer's LAN address
   comes first; the `.local` name works on most home and office networks too.
2. Name the device, for example `Sam's phone`. The list shows the name, and
   the take history records the id made from it, `sam-s-phone`.
3. Choose what it may do. **Operate** is the default: take shots, switch
   scenes, run the show. **Read** watches only. **Admin** can change settings
   and make tokens for other devices, so keep it for your own devices.
4. Press **Make a code** and scan the QR code with the phone's camera.

The code opens `https://<address>:<port>/#token=<token>`. The token is in the
part after `#`, which a browser never sends to the server, so it is in no log
on the way. The page stores it and takes it off the address bar before it
draws, and the phone is signed in.

Each code is a new token. Making a second code for the same phone gives it a
second entry in the list, which is harmless; revoke the one you do not need.

The card also has **Copy link**, for a device with no camera to scan with.
Anyone who opens that link is signed in as that device, so send it only to
the device it is for.

If the card says the mixer only answers on the machine it runs on, step 1 is
not done yet. The card says which setting to change.

## 3. Accept the certificate on the phone

The mixer makes its own certificate the first time it starts, for
`localhost`, the computer's name, its `.local` name and its LAN addresses
(see [Serve the control port over HTTPS](serve-https.md)). No public
authority signed it, so every phone warns once. The card shows the
certificate's fingerprint; it is the same one the mixer prints when it
starts, and a phone that shows the same fingerprint is talking to your mixer.

**Android, Chrome:** the page says *Your connection is not private*. Tap
**Advanced**, then **Proceed to 192.168.1.20 (unsafe)**. Chrome remembers the
choice for that address.

**iPhone and iPad, Safari:** the page says *This Connection Is Not Private*.
Tap **Show Details**, then **visit this website**, and confirm. Safari
remembers it for that address.

If Safari will not let you past, or you want the warning gone for good,
install the certificate on the iPhone:

1. Find the certificate file on the computer. It is beside the mixer's
   config, named after it: `godwinmix.control.crt` for `godwinmix.toml`. In
   the desktop app, **Open config folder** in the GodwinMix menu goes there.
2. Send it to the iPhone by AirDrop or as an email attachment, and open it.
   iOS says *Profile Downloaded*.
3. Open **Settings > General > VPN & Device Management**, tap the profile and
   **Install**.
4. Open **Settings > General > About > Certificate Trust Settings** and turn
   on full trust for the GodwinMix certificate.

The mixer makes a new certificate when the computer's LAN address changes and
a month before the old one runs out. The fingerprint then changes and each
phone asks again; an installed certificate has to be installed again.

## Firewalls

The control port has to be reachable from the phones. That is the one TCP
port shown in the address, the port the desktop app keeps, or 8080 on a
server.

* **Windows:** the installer allows the mixer and each plugin it carries on
  private and domain networks, so Windows does not ask. The rules are in the
  group *GodwinMix* in *Windows Defender Firewall with Advanced Security*,
  one per program, and the uninstaller removes them. They are written for the
  person who ran the installer, because the app runs its plugins from that
  person's AppData; another account on the same computer is asked once per
  program. Windows still asks on a network it calls public. If this is your
  own network, make it private under *Settings > Network & internet*, choose
  the connection, then *Private network*. If you dismissed the question,
  allow the program under *Windows Security > Firewall & network protection
  > Allow an app through firewall*. A plugin installed later, from a
  marketplace or by hand, asks once.
* **macOS:** if the firewall is on, macOS asks whether to accept incoming
  connections for the mixer. Allow it.
* **Linux:** with `ufw`, `sudo ufw allow 8080/tcp`, using your port.

A phone that sends its own camera to the mixer over WHIP also needs UDP 8189
to 8204 (one port per phone that is sending, from the ingest plugin's
`webrtc_port`). Running the show from a phone needs only the control port.

A guest or hotel network often keeps devices from reaching each other at
all. If a phone cannot open the address and the computer can, try a network
you control, or the computer's own hotspot.

## Add a phone's camera

A phone can also be one of the cameras. It needs no operator token for that:
press **Add a source**, then **Cameras**, then **Show code** on **A phone's
camera**, and scan that code with the phone. Each phone that scans it becomes
a source of its own, named after the phone or after the name typed on its
page. The same phone can be a camera and a control surface at once, in two
browser tabs, though the camera tab has to stay in front.

The full steps, the camera flip, holding the phone upright or on its side, and
keeping the screen awake are in
[Use this browser's camera: Add a phone's camera](use-this-browsers-camera.md#add-a-phones-camera).

## Take a device back

**Help > Open on another device** lists every device token with its name,
what it may do and when it was made. **Revoke** signs that device out: its
next call is refused, even on a page it already has open, and the token cannot
be used again. A phone that should come back scans a new code.

The same list is there for a script or an agent, as `token.list` and
`token.revoke` (see [Device tokens](../reference/device-tokens.md)):

```sh
gmx ctl rpc token.list '{}'
gmx ctl rpc token.revoke '{"id": "sam-s-phone"}'
```

## When it does not work

**The phone cannot open the address.** Check the setting from step 1 is on,
that the phone is on the same network as the computer, and the firewall
section above. Opening the same address in a browser on the computer itself
tells you whether the mixer is answering on it.

**The page asks the phone for a token.** The token in the code was revoked,
or it was made on another mixer. Make a new code.

**The phone signs in but cannot take.** Its token is `read`. Revoke it and
make an `operate` code instead.

**A take from the phone is refused with a hold or rate message.** That is the
mixer's own safety guard, which applies to every token. See
[Safety](../reference/safety.md).
