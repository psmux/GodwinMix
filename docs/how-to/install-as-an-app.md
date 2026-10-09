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
worker. Chrome's DevTools give the installability error as
`not-from-secure-origin`, and the page's console says the worker failed with
"An SSL certificate error occurred when fetching the script." A phone that
opens the mixer's `https://` address meets exactly that until it has been
told to trust the mixer, which takes a minute and is done once
([trust this mixer on a phone](#trust-this-mixer-on-a-phone), below).

| Where | Address | Install offered | Offline page |
|---|---|---|---|
| Chrome or Edge on the mixer's machine | `http://localhost:8080` | yes | yes |
| Chrome or Edge, another machine | `http://192.168.1.20:8080` | no | no |
| Chrome or Edge, another machine | `https://` with the warning clicked through | no | no |
| Chrome or Edge, another machine | `https://` with the mixer trusted | yes | yes |
| Chrome on Android | the same as Chrome above | the same | the same |
| Safari on iPhone or iPad | any of them | Add to Home Screen | only with the mixer trusted |
| the desktop app | | it is an app already | not used |

Port 8080 is the default. Use the one your mixer printed when it started.

## Install it

**Chrome or Edge on a desktop.** Open the mixer's address. An install button
appears at the right of the address bar; press it. The browser's menu has the
same thing: Chrome keeps it under Cast, save and share, and Edge under Apps.

**Chrome on Android.** Open the address and go to More, the last tab along
the bottom. Under This mixer is Install app, which brings up Chrome's own
install dialog. Until the phone trusts the mixer the row reads Trust this
mixer on your phone first, and opens the steps below. The browser's menu
still has Add to Home screen, but without a secure page that makes a
shortcut which opens in an ordinary Chrome tab.

**Safari on iPhone or iPad.** Open the address and tap Share, then Add to
Home Screen. More shows a line saying so. Safari does this for any page,
http included, and the icon opens the mixer full screen. Two catches. Over
`https://` with a certificate warning you only clicked through, the app from
the home screen has no way to show the warning again and may refuse to load
the page at all, so either use `http://` or trust the mixer first. And until
the mixer is trusted there is no offline page: when the mixer is away the app
shows Safari's own error.

The Install app row is not shown when the page is already open as an
installed app, inside the desktop app, on the mixer's own machine where the
browser has not offered, or where the mixer has no authority of its own to
trust. There is never a button that does nothing.

## Trust this mixer on a phone

The mixer makes a small certificate authority of its own the first time it
starts, once per machine, and signs the certificate on its https port with
it. Install the authority on a phone once and the phone trusts the mixer:
no warning, and Chrome on Android can offer to install the page. When the
mixer later makes a new certificate, because the machine got a new address
or the old certificate is a month from running out, the authority stays the
same and the phone goes on trusting it. The authority itself lasts ten years.

The mixer says where to get it when it starts, under the https line. From a
run on port 18650:

```text
HTTPS is on the same port: https://192.168.77.173:18650/ (certificate fingerprint 45:BC:C5:5D:18:0A:0F:59:40:89:1F:B0:A5:70:17:CE:5C:6E:22:0B:77:6F:21:85:82:F7:81:BE:08:32:FE:71).
To trust it on a phone, install its authority from https://192.168.77.173:18650/ca.crt (fingerprint 2E:62:B7:DC:1D:A5:1C:D1:34:7F:7E:D2:6C:C0:D5:01:BF:15:C7:5F:F3:AE:6C:F0:F2:FB:94:5D:78:CF:6D:1D).
```

The same addresses and the authority's fingerprint are in Help, Open on
another device, under Trust this mixer on your phone. The authority's
certificate is public, so downloading it needs no token. Its private key
stays sealed in the mixer's secret store and never leaves the machine.

**iPhone or iPad.** In Safari, open the mixer's address followed by
`ca.crt`, for example `https://192.168.77.173:18650/ca.crt`, and accept the
certificate warning for that one page (`http://` works for the download too).
Safari asks whether to allow a configuration profile; allow it. Then:

1. Settings, Profile Downloaded, Install, and enter the passcode.
2. Settings, General, About, Certificate Trust Settings, and switch on
   GodwinMix local authority on the mixer's machine name.

**Android.** In Chrome, open the mixer's address followed by `ca.pem`, for
example `https://192.168.77.173:18650/ca.pem`. It downloads as
`godwinmix-authority.crt`. Android will not install an authority straight
from a browser, which is why this one is a plain download. Then:

1. Settings, Security, Encryption and credentials, Install a certificate,
   CA certificate.
2. Accept the warning and pick `godwinmix-authority.crt` from Downloads.

Phone makers move these settings about a little; if the path is not there,
search Settings for "certificate".

Then open the mixer's `https://` address again. There should be no warning,
and on Android, More should have Install app.

Compare the fingerprint the phone shows for the authority with the one the
mixer printed before you switch it on.

### What has been checked, and what has not

The phone steps above are Apple's and Google's own settings paths. They have
not been run on a phone for this page. What was checked, on Windows, against
a running mixer:

* `openssl x509 -in godwinmix.control.crt -noout -ext basicConstraints,keyUsage`
  prints `CA:TRUE, pathlen:0` and `Digital Signature, Certificate Sign, CRL Sign`.
* The certificate the port serves says `CA:FALSE`, names the authority as its
  issuer, covers `localhost`, `127.0.0.1`, `::1`, the machine's name and its
  LAN address, and `openssl verify -CAfile godwinmix.control.crt` passes for it.
* `/ca.crt` is the same file as `godwinmix.control.crt`, served as
  `application/x-x509-ca-cert`; `/ca.pem` is the same again as a download.
* Headless Edge, told to trust only the authority's key
  (`--ignore-certificate-errors-spki-list`), opened the `https://` LAN
  address with no certificate error, registered the service worker, reported
  no installability errors, and fired the install offer, so More showed
  Install app.

To stop trusting the mixer, remove the profile on iOS (Settings, General,
VPN and Device Management) or the certificate on Android (Settings, Security,
Encryption and credentials, User credentials).

### Or bring your own certificate

A mixer with a real name can have a real certificate from a
[reverse proxy](reverse-proxy.md) such as Caddy, which gets one from Let's
Encrypt, and then nothing has to be installed on any phone. A certificate
from an authority you already run works too: set `[control.tls] cert` and
`key` ([serve the control port over HTTPS](serve-https.md)) and install your
authority on the phones the same way. The mixer then has no authority of its
own to offer, and `/ca.crt` answers 404 saying so.

## What the app keeps, and what it does not

The service worker answers for the page's own files only: `index.html`, the
modules under `client/`, `shell/`, `panels/` and `kits/`, the stylesheets, the
icons and the manifest. It asks the mixer for each one first and keeps a copy,
and uses the copy only when the mixer cannot be reached at all.

It never answers for, and never keeps, anything else: `/api/`, `/rpc`, the
WebSocket, `/mjpeg/`, `/pcm/`, `/opus/`, `/whep/`, `/whip/`, `/hls/`,
`/metrics`, `/mcp`, `/plugins/` and `/presets/` go straight to the mixer as
though the worker were not there. Where the browser has static routes
(Chrome 123 and later), the worker also asks it to send those paths straight
to the network without waking the worker at all.

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
