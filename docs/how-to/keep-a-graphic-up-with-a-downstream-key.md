# Keep a graphic up with a downstream key

Put a logo, a lower third or a ticker on air once and keep it there through
every take, without copying it into every scene. A downstream key is an
ordinary scene drawn above whatever is on air, a scene or a single source, until
you take it down.

## From the page

1. Make a scene with only the graphic in it, in the place it should sit: a
   logo in a corner, a name bar along the bottom.
2. Right click that scene in the **Scenes** panel and choose **Put up as a key,
   above every take**. It fades in over what is on air.
3. Take scenes and sources as usual. The key stays above each of them.
4. Right click the same scene and choose **Take the key down** to fade it out.

Several keys can be up at once. They are drawn in the order they went up, the
first lowest.

## Over the API

```sh
gmx ctl rpc program.key.set '{"scene":"Logo","on":true}'
gmx ctl rpc program.key.set '{"scene":"Logo","on":false,"transition":"cut"}'
gmx ctl rpc program.key.list
```

`transition` takes what `program.take` takes; left out it is a 300 ms fade. An
agent does the same with the `set_downstream_key` tool.

## What to know

* Putting a key up or down is not a take. It is not held to the safety rules
  an agent's takes are held to, and it is not in the take history, so
  `program.revert` goes back to the shot before and keeps the key up.
* The keys that are up are kept in the scene collection, so a restart brings
  them back over what was on air.
* A camera inside a key is on air as far as tally is concerned.
* The preview shows the scene armed without the keys.
* Taking the key's own scene draws it once, not twice.
