# icecast

Internet radio, both ways.

| Provide | What it does |
|---|---|
| `icecast/output` | the programme's sound to an Icecast or SHOUTcast 2 mount, as MP3, Ogg Vorbis or Ogg Opus |
| `icecast/source` | a radio station, or any audio stream over HTTP, played as a live source, with its song titles in health |

A church that streams its service on video usually also has people who only
want to listen: in the car, on a phone with poor signal, on a smart speaker.
An Icecast mount serves them at a tenth of the bandwidth, and every radio app
plays it.

How to use it from the page is [docs/how-to/radio.md](../../docs/how-to/radio.md).
Every setting is in [docs/reference/plugins-network.md](../../docs/reference/plugins-network.md#icecastoutput-and-icecastsource).

## Build and install

```sh
./build                            # stage bin/gmx-icecast
gmx plugin add ./plugins/icecast   # runs ./build itself when bin/ is empty
```

## Tested

`cargo test -p gmx-icecast` sends 4 s of programme to an Icecast server run by
the test, which checks the source login and the mount and keeps what it is
sent; the MP3 it received is decoded and measured. The source plays a station
run by the test that streams MP3 with ICY song titles; the title is read and
the sound decoded.

Not tested here: a real Icecast or SHOUTcast server, and the Ogg formats end
to end.
