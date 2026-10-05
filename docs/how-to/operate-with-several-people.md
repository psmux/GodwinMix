# Operate one mixer with several people

A show often has more than one pair of hands on it: somebody at the desk on a
laptop, somebody on the floor with a phone, a third person laying out the next
scene on a tablet. They can all have the page open on the same mixer at once,
on the same token or on different ones, and nobody's work is undone or
overwritten by somebody else's without being asked first.

## Open the page on each device

Point each browser at the mixer's address, the same one the desk uses, and
give it the token if the mixer has one. Nothing else is set up. Each browser
tab is its own client: the page picks a name for the tab and keeps it while the
tab is open, so a reload or a dropped Wi-Fi connection comes back as the same
client.

## See who else is here

When anybody else has the page open, the header shows how many: **1 other
here**, **2 others here**. Tap it for the list. Each line is a person's device
as their browser describes it ("iPhone Safari", "Windows Edge"), or the name
they gave it, and the scene they are editing when they are in the composer.

The composer shows the same thing for its own scene. If somebody else opens
the scene you are editing, a marker in the composer's bar says so, for example
"iPhone Safari is editing this scene too", and goes away when they close it.

## Undo your own changes

Ctrl+Z, or Undo in the Edit menu, takes back your last change, and only yours.
If the person on the phone moved a box after you renamed a scene, your undo
puts the name back and leaves their box where they put it.

If undoing your change would overwrite something somebody else did to the same
item since, the page asks first:

> Somebody else changed this after you: item "lower third" (changed by iPhone
> Safari). Undo anyway and put your version back over theirs?

Say no and nothing changes; your step stays, so you can change the item by hand
instead. Say **Undo anyway** and your version goes back over theirs.

## Apply a scene somebody changed while you had it open

The composer edits a copy of the scene. If somebody changes the real scene
while your copy is open, Apply does not quietly write over them. It asks:

* **Keep editing** goes back to your copy.
* **Start again from the scene** throws your copy away and opens a fresh one
  from the scene as it is now, with their change in it.
* **Apply anyway** replaces the scene with your copy, their change included.

A take of that scene does not apply a copy that has gone stale either. The take
goes to air with the scene as it is, and your copy stays open for you.

## From a script or the command line

A script on `/rpc` gets a client id of its own by connecting. To keep the same
one across reconnects, name it on the URL:

```sh
websocat "ws://mixer.local:8080/rpc?token=$TOKEN&client_id=lower-thirds"
```

Over HTTP, put `client_id` in the params of every call that should count as
yours:

```sh
curl -X POST http://mixer.local:8080/api/v1/scenes/undo \
  -H "Authorization: Bearer $TOKEN" -H 'content-type: application/json' \
  -d '{"client_id": "lower-thirds"}'
```

Without one, every HTTP caller on a token is the same client and shares one
undo stack, which is how `gmx ctl scene undo` behaves today.

To see who is connected: `gmx ctl rpc presence.list`. To tell the others which
scene a script is working on, call `presence.set {"scene": "wide"}` on its
`/rpc` connection. [Presence](../reference/presence.md) has every field, and
[Scene commands](../reference/scene-commands.md) has the refusals and their
`data`.

## What this does not do yet

Two people dragging the same box at the same moment each see their own drag
until they let go, and the one whose move reached the mixer last wins. There is
no lock on an item. A token can carry a label of its own only where tokens are
minted at run time; a token from the config file shows its connections by
device.
