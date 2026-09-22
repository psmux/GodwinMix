# Change a setting

The canvas size, the programme bitrate, the multiview and snapshot switches,
the media folder, the safety rules and the rest of the mixer's own settings
live in its config file, `godwinmix.toml`. You can change them over the
control API without opening the file, and the file keeps every comment you
wrote in it.

The web UI does not have a Mixer settings dialog yet. It will be built on the
methods below; until then, use the API from a terminal as shown here.

## See what is set

```sh
curl -s -H "authorization: Bearer $GODWINMIX_TOKEN" \
  http://127.0.0.1:8080/api/v1/config
```

Each key comes back with its value in the file (or its default, when the file
does not set it), where the value came from, and `applies`: when a change to
it takes effect. The control token is never sent back; its row only says
whether one is set.

## Change one

```sh
curl -s -X POST -H "authorization: Bearer $GODWINMIX_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"values": {"program.video_bitrate_kbps": 3000}}' \
  http://127.0.0.1:8080/api/v1/config/set
```

Send as many keys as you like in `values`. Either all of them are written or
none are: a value out of range, of the wrong type, or one that would stop the
file loading (an odd canvas width, say) is refused with the reason and the
range or choices that would work, and the file is not touched.

Look at the answer before you walk away:

* `applied` lists the keys in force now. The safety rules, the stall policy,
  the audio ramp on a take and `security.allow_exec_sources` are like this.
* `next_source` lists the keys the next source you add or rebuild will use.
  The `[browser]` settings are like this; a web page already on air keeps the
  browser it was started with.
* `needs_restart` lists every key still waiting for the mixer to be started
  again, from this change or an earlier one. The canvas, the programme
  encoder, the multiview, snapshots, the media folder and the control address
  are like this, because they are built once when the mixer starts.

Add `"dry_run": true` to see that split without writing anything.

## Put one back to its default

```sh
curl -s -X POST -H "authorization: Bearer $GODWINMIX_TOKEN" \
  -H 'content-type: application/json' \
  -d '{"keys": ["program.video_bitrate_kbps"]}' \
  http://127.0.0.1:8080/api/v1/config/reset
```

This takes the key out of the file, so the built in default applies.

## Change the control token

Send the new token as `control.token`. It is written to the file and takes
effect on the next start, so keep a copy before you restart or you will not
be able to reach the mixer. An empty string removes the token, which opens
the control port to anyone who can reach it. If the mixer was started with
`GODWINMIX_TOKEN` set, that wins over the file, and the answer says so.

## What cannot be changed here

Sources, outputs and filters have their own methods (`source.set`,
`output.set`, `filter.set`); a plugin's settings are `plugin.settings.set`;
the `[[tokens]]` table and `[codecs]` are still edited in the file. A
`config.set` naming one of those is refused with the method to use instead.

Every key, its type and its range: [the config methods](../reference/config-methods.md).
