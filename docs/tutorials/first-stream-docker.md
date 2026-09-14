# Your first stream in five minutes, with Docker

By the end of this you will have a web page on air, going out over RTMP, and
you will have watched it play back. Nothing is installed on your machine except
Docker.

Five minutes is the claim and it is timed in CI on every push, on a runner with
no GPU. If the number in the
[quickstart job](https://github.com/psmux/GodwinMix/actions/workflows/quickstart.yml)
is over five minutes, the claim is wrong and the job is red.

What you need: Docker with Compose, and a terminal. No stream key, no account.

## 1. Get the repository

```sh
git clone https://github.com/psmux/GodwinMix
cd GodwinMix
```

## 2. Start the mixer and an RTMP server

```sh
docker compose -f deploy/docker/docker-compose.yml up -d --build
```

Two containers come up. `godwinmix` is the mixer, on port 8080. `rtmp` is
mediamtx, which is standing in for YouTube: it accepts the programme over RTMP
on 1935 and hands it back as HLS on 8888, so you can see what went out.

The first build compiles the mixer from source and takes a few minutes. That is
not counted in the five, because a person doing this for real pulls the
published image:

```sh
docker pull ghcr.io/psmux/godwinmix:latest
```

Watch it come up:

```sh
docker compose -f deploy/docker/docker-compose.yml logs -f godwinmix
```

Wait for the line that says the control server is listening, then press Ctrl+C.
That stops following the log, not the container.

## 3. Open the UI

<http://localhost:8080>. It asks for a token; the token is `change-me`, which
is what the compose file passes in as `GODWINMIX_TOKEN`. Change it before this
port is reachable from anywhere but your own machine.

You are looking at an empty mixer: black on programme, no sources, no outputs.

## 4. Tell it where the programme goes

Every call needs the token, so set up a small shell function first.

```sh
export TOKEN=change-me
api() { curl -sf -X "$1" "http://localhost:8080$2" \
          -H "Authorization: Bearer $TOKEN" \
          -H 'content-type: application/json' ${3:+-d "$3"}; }
```

Add the destination:

```sh
api POST /api/outputs '{"id":"primary","uri":"rtmp://rtmp:1935/live/program","policy":"own"}'
```

`rtmp` is the hostname of the mediamtx container on the compose network. The
output goes live within a second or two, carrying black and silence. That is
the point: the programme starts before there is anything to show and never
stops after that.

Check:

```sh
api GET /api/outputs
```

## 5. Add a page as a source

```sh
api POST /api/sources '{"id":"page","uri":"web+https://example.com/","name":"A page"}'
```

A browser starts inside the container and renders the page off screen. It takes
5 to 20 seconds before the source reports `live`. Watch for it:

```sh
api GET /api/status | jq '.sources[] | {id, state}'
```

Do not take a source that is not `live`. It will not break anything, but you
will be looking at black and wondering why.

## 6. Put it on air

```sh
api POST /api/take '{"source":"page"}'
```

Or click its cell in the UI. The take lands on the next frame. The output does
not reconnect, the encoder does not restart, and anyone watching sees a cut.

## 7. Watch what went out

Open <http://localhost:8888/live/program> in a browser, or:

```sh
ffplay rtmp://localhost:1935/live/program
```

There is a few seconds of HLS latency. The page you added is on screen.

## 8. Stop

```sh
docker compose -f deploy/docker/docker-compose.yml down
```

## What just happened

The output encoder started when you added the output and ran continuously from
that moment. Adding a source built a second, separate pipeline; taking it
changed a property on a compositor pad inside the programme pipeline. Nothing
downstream of the compositor was touched, which is why nothing downstream
noticed. [Why the programme never stops](../explanation/why-the-programme-never-stops.md)
is the long version.

## Next

* Point it at a real destination: replace `rtmp://rtmp:1935/live/program` with
  your YouTube or Twitch ingest address and stream key.
* Add a camera: anything publishing RTMP to `rtmp://localhost:1935/live/cam1`
  becomes a source at that address.
* [Run it on a server](../how-to/headless-server.md) properly, with a token
  that is not `change-me` and TLS in front of it.

## If it did not work

| What you see | What it is |
|---|---|
| The UI does not load | The container is not up. `docker compose ps`, then `docker compose logs godwinmix`. |
| 401 on every call | The token. `export TOKEN=change-me`, or whatever you put in `GODWINMIX_TOKEN`. |
| The source never leaves `starting` | The page renderer. `docker logs godwinmix` and look for WPE or WebKit errors; [the container notes](../../deploy/docker/README.md) list the ones that are known. |
| The output stays `connecting` | mediamtx is not up, or the address is wrong. From inside the mixer container the RTMP server is `rtmp`, not `localhost`. |
| Programme is black after a take | The source was not `live` when you took it. Check `api GET /api/status` and take it again. |

Anything else, and especially anything where the message did not tell you what
to do next: that is a bug in the message. Open an issue, and add the moment to
[the friction log](../friction-log.md).
