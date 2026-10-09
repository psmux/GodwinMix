# Install the control page as an app

The page a phone or a laptop opens to run the mixer can be installed like an
app. It then has its own icon on the home screen or in the dock, opens
without the browser's address bar, and when the mixer cannot be reached it
says so and what to check, rather than showing the browser's error.

Installing changes nothing on the mixer. It is the same page from the same
address, and the mixer is asked for every file each time it opens, so an
updated mixer is what you see the next time the app starts.

## What you need first

Every browser except Safari on iOS insists on a secure page before it offers
to install one. A page is secure when it is

* `http://localhost` or `http://127.0.0.1`, on the mixer's own machine, or
* `https://` with a certificate the device trusts.

A certificate warning you clicked through does not count. The browser shows
the page, but it will neither offer to install it nor run the page's service
worker, and it says why in DevTools: the page "is not served from a secure
origin". This is the case on a phone opening the mixer's own self signed
certificate, so the steps below for a phone start with making it trusted.

| Where | Address | Install offered | Offline page |
|---|---|---|---|
| Chrome or Edge on the mixer's machine | `http://localhost:8080` | yes | yes |
| Chrome or Edge, another machine | `http://192.168.1.20:8080` | no | no |
| Chrome or Edge, another machine | `https://` with the warning clicked through | no | no |
| Chrome or Edge, another machine | `https://` with a trusted certificate | yes | yes |
| Chrome on Android | the same as Chrome above | the same | the same |
| Safari on iPhone or iPad | any of them | Add to Home Screen | only with a trusted certificate |
| the desktop app | | it is an app already | not used |

Port 8080 is the default. Use the one your mixer printed when it started.

## Install it

**Chrome or Edge on a desktop.** Open the mixer's address. An install button
appears at the right of the address bar; press it. The browser's menu has the
same thing: Chrome keeps it under Cast, save and share, and Edge under Apps.

**Chrome on Android.** Open the address and go to More, the last tab along
the bottom. Under This mixer is Install app, which brings up Chrome's own
install dialog. If there is no such row, Chrome has not offered to install
the page, which almost always means the certificate is not trusted yet. The
browser's menu still has Add to Home screen, but without a secure page that
makes a shortcut which opens in an ordinary Chrome tab.

**Safari on iPhone or iPad.** Open the address and tap Share, then Add to
Home Screen. More shows a line saying so. Safari does this for any page,
http included, and the icon opens the mixer full screen. Two catches. Over
`https://` with a certificate you only clicked through, the app from the home
screen has no way to show the warning again and may refuse to load the page
at all, so either use `http://` or trust the certificate first. And with no
trusted certificate there is no offline page: when the mixer is away the app
shows Safari's own error.

The Install app row is not shown when the page is already open as an
installed app, inside the desktop app, or where the browser has not offered
to install and is not on iOS. There is never a button that does nothing.

## Make a phone trust the mixer's certificate

The mixer writes its certificate beside its config file as
`<config name>.control.crt` ([serve the control page over https](serve-https.md)).
It is self signed and made for one server, and it is not a certificate
authority. Phones only let a person trust a certificate authority:

* iOS lists a certificate under Settings, General, About, Certificate Trust
  Settings only when it is a certificate authority, so the mixer's own
  certificate can be installed as a profile but never switched on there.
* Android installs a certificate as a trusted one under Settings, Security,
  Encryption and credentials, Install a certificate, CA certificate, and that
  wants a certificate authority too.

`openssl` shows whether a certificate is one. The mixer's own has no line
saying `CA:TRUE`:

```sh
openssl x509 -in godwinmix.control.crt -noout -text | grep -A1 "Basic Constraints"
```

So for an installable app on a phone, give the mixer a certificate from a
certificate authority the phone trusts. Two ways that work:

1. **A small authority of your own, with mkcert.** On any computer:

   ```sh
   mkcert -install
   mkcert 192.168.1.20 mixer.local localhost 127.0.0.1
   ```

   That makes `192.168.1.20+3.pem` and `192.168.1.20+3-key.pem`. Point the
   mixer at them:

   ```toml
   [control.tls]
   cert = "192.168.1.20+3.pem"
   key = "192.168.1.20+3-key.pem"
   ```

   and restart it. Then put mkcert's authority on the phone. `mkcert -CAROOT`
   prints the folder; the file is `rootCA.pem`. Send it to the phone (mail,
   AirDrop, a USB cable), then

   * on iOS, open it, install the profile under Settings, General, VPN and
     Device Management, and switch it on under Settings, General, About,
     Certificate Trust Settings;
   * on Android, Settings, Security, Encryption and credentials, Install a
     certificate, CA certificate, and pick the file. Chrome trusts
     authorities a person installed this way.

   Keep `rootCA-key.pem` to yourself: anyone with it can make certificates
   every device that trusts it accepts.

2. **A real name with a real certificate**, from a
   [reverse proxy](reverse-proxy.md) such as Caddy, which gets one from
   Let's Encrypt. Nothing has to be installed on the phones at all.

Use the address the certificate names. A certificate for `192.168.1.20` is
no use to a phone that opens `mixer.local`.

## What the app keeps, and what it does not

The service worker answers for the page's own files only: `index.html`, the
modules under `client/`, `shell/`, `panels/` and `kits/`, the stylesheets, the
icons and the manifest. It asks the mixer for each one first and keeps a copy,
and uses the copy only when the mixer cannot be reached at all.

It never answers for, and never keeps, anything else: `/api/`, `/rpc`, the
WebSocket, `/mjpeg/`, `/pcm/`, `/opus/`, `/whep/`, `/whip/`, `/hls/`,
`/metrics`, `/mcp`, `/plugins/` and `/presets/` go straight to the mixer as
though the worker were not there. Chrome 123 and later skip the worker for
those before it even starts.

When the app opens and the mixer is not answering, it shows a page that says
so and lists what to check: that the mixer is running, that the device is on
the same network with no VPN in the way, and that the address is still the
mixer's. It tries again every ten seconds.

An installed app opens the address it was installed from. If the mixer's
machine gets a new address on the network, open the new one in the browser
and install again.

## Remove it

Remove it the way you remove any app: press and hold the icon on a phone, or
in Chrome or Edge open the app's menu and choose Uninstall. To drop the
offline copy as well, clear the site's data in the browser's settings for the
mixer's address.

## What is not here yet

There is no Install app entry on the desktop layout, which a tablet held
sideways also gets. Use the browser's own install button there.
