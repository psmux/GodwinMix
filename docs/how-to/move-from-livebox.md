# Move your channels from Livebox

Your encoders already publish to Livebox, each one to an address like this:

```
rtmp://192.168.1.10:1935/Church/main?psk=Sunday-2024
```

`Church` is the Livebox channel name, `main` the stream name and
`Sunday-2024` the password. GodwinMix can make a channel that answers to that
exact address and password, so the encoder settings stay as they are. Only the
host changes, and not even that if the mixer takes over the Livebox machine's
address.

## Before you start

Install the ingest plugin if the Channels tab asks for it. Its RTMP port is
1935 unless `rtmp_port` under `[plugins.ingest]` says otherwise, which is the
port Livebox used, so an encoder pointed at the mixer's address needs nothing
else changed.

Have the addresses to hand. Each Livebox channel's dashboard shows a STREAM URL
and a STREAM KEY; an encoder's own settings show the same thing as one address
or as a server and a key.

## One channel by hand

1. On the Channels tab press **Add Channel**.
2. Type a name. It can be the Livebox channel name.
3. Open **More options**.
4. In **Address** type the Livebox channel name exactly as the encoders send
   it, capitals included: `Church`. The line above shows the address the
   encoders will publish to.
5. In **Password** type the password the encoders send after `?psk=`.
6. Press **Create channel**.

The card that opens shows the key as the encoders send it. In the channel's
settings the key is marked **Typed**, so later you can tell it from the keys
the mixer made.

A password has to be 6 to 128 letters, digits, dashes, underscores, dots,
tildes or spaces. If yours is shorter, or has another character in it, the
mixer says which rule it broke and puts the cursor in the box. You then have
to change the password on the encoders too, there is no way round that.

## Several at once

Press the **⋯** button beside Add Channel and choose **Bring channels from
Livebox**, or press Ctrl+K and type `Livebox`. Paste one address a line:

```
rtmp://192.168.1.10:1935/Church/main?psk=Sunday-2024
rtmp://192.168.1.10:1935/Youth/main?psk=youth-night
rtmp://192.168.1.10:1935/Choir/rehearsal?psk=choir-pw
```

The dashboard's two fields work too, pasted one under the other, with or
without their labels:

```
STREAM URL
rtmp://192.168.1.10:1935/Church/
STREAM KEY
main?psk=Sunday-2024
```

Press **Bring them in**. Each line becomes one channel, made through
`channel.add` the same way Add Channel makes one, and gets a line of its own
under the box:

* **made**: the channel is there, and its encoders are let in.
* **already the channel**: this mixer has a channel with that address. Press
  **Add this password to** it and the password becomes one more key on that
  channel, labelled Livebox.
* anything else is the mixer's own sentence, starting with the line number it
  came from, for example a line with no `?psk=` on it or a password that is
  too short.

Nothing on the page shows a password back.

## A channel you already made

Open the channel's settings. Under **Keys**, open **Use a password you already
have**, type it, and press **Make a key**. The channel now takes both its old
keys and that password.

## From a terminal or an agent

The paste box calls the same public methods anything else can call. From a
terminal:

```sh
gmx ctl rpc channel.add '{"name":"Church","app":"Church","secret":"Sunday-2024"}'
gmx ctl rpc channel.key.add '{"id":"church","label":"Livebox","secret":"another-pw"}'
gmx ctl rpc channel.list
```

The last one prints every channel as JSON, its keys by label and last four
characters, never the keys themselves. `gmx` talks to the mixer named by
`--url` or `GODWINMIX_URL`, with `--token` or `GODWINMIX_TOKEN`; given
neither, it tries the desktop app's mixer, then `http://127.0.0.1:8080`.

An agent sees `secret` among the `add_channel` tool's parameters. The method
reference is [The channel methods](../reference/channels.md).

## Capitals and spaces

The mixer keeps the address as you typed it, `Church` and not `church`, but
it does not care about case when an encoder arrives: `church`, `Church` and
`CHURCH` all reach the same channel. That is also why two channels cannot have
addresses that differ only in case.

Livebox allowed a space in a channel name. OBS and ffmpeg stop reading an RTMP
address at a space, so an encoder publishing to such a channel has to send it
as `%20`: `rtmp://192.168.1.10:1935/Youth%20Hall/main?psk=...`. The mixer
shows every address that way and decodes it when the encoder arrives. If an
encoder was set up with a plain space and worked on Livebox, check what it
really sends before you rely on it; the channel name without the space is the
simpler fix.

The channel's id stays a lower case slug (`church`, `youth-hall`), which is
what the sources it makes are named after.

## What is called what

| Livebox | GodwinMix |
|---|---|
| Create channel | Add Channel |
| Channel Name | Address, under More options |
| Password | Password, under More options, and a key of the channel |
| Stream URL | Server |
| Stream key | Stream key |
| Push Destinations | Send on to |
| Channel Dashboard | the Channels tab |

## What does not come across

The paste box makes channels and keys. Livebox's push destinations are not in
the address, so add them again on the channel's card under **Send on to**.

The Livebox stream name is not kept anywhere. A GodwinMix channel takes any
stream name, so `main`, `rehearsal` and the rest keep working without being
set up.
