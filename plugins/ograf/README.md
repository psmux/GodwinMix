# gmx-ograf

Words on the picture.

An [OGraf](https://ograf.ebu.io/) graphic is a `graphic.ograf.json` saying what
words go in a template, beside a web component that draws them. The EBU
specified it, which is why this is a host for that format rather than a
template format of its own: the templates already exist, and one written for
another OGraf host plays here.

This plugin serves each placement as its own page. The mixer renders that page
as an ordinary source, so the compositor never learns what a graphic is.

## In ten minutes, from nothing

**1. Build and install it**

```sh
cargo build --release -p gmx-ograf
dev/plugins.sh build --release
gmx plugin add ./plugins/ograf
```

**2. Put one on a scene**

```sh
gmx ctl scene item add "Wide" --graphic ograf/lower-third --name "speaker strap"
```

**3. Put words in it and play it on**

```sh
gmx ctl graphic apply ograf/lower-third --set name="Ada Lovelace" --set title=Analyst --play
```

## Writing a graphic

```sh
gmx plugin new --kind graphic my-strap
cd my-strap && ./check
```

You get a manifest, a web component with the four OGraf methods on it, and a
preview you can open in a browser while you work:

```sh
gmx-ograf --serve --root .
open 'http://127.0.0.1:7841/graphic/my-strap/my-strap?values={"name":"Ada"}&play=1'
```

`docs/how-to/make-a-graphic.md` is the long version.

## What it serves

| Route | What it is |
|---|---|
| `/` | what it can serve and what it is driving, for a person |
| `/health` | `{ok, graphics, instances}` |
| `/graphic/<plugin>/<id>?instance=<i>` | the page a browser source loads |
| `/graphic/<plugin>/<id>/manifest.json` | the OGraf manifest |
| `/graphic/<plugin>/<id>/<file>` | the graphic's own files |
| `/state/<instance>` | what that placement is showing now |
| `/events/<instance>` | the actions, as `text/event-stream` |

It binds 127.0.0.1 and nothing else. A graphics host reachable from the network
is a way to put words on somebody else's programme.

## The settings

| Key | Default | What it does |
|---|---|---|
| `port` | 7841 | The port on the loopback. 0 takes any free one, which is what to use when two mixers share a machine. A port already taken falls back to a free one and the log says which. |

## What transparency costs

The mixer graph is I420, which carries no alpha. A graphic that is an opaque
shape is right on air today; one with a soft edge or a gap you should see the
camera through is not. `docs/reference/graphics.md` says what the change is.
