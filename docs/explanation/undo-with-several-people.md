# Undo when several people edit one show

The scene document has one copy, in the core, and every client edits it with
commands. Undo is a stack of inverse patches kept beside it. Until clients had
ids of their own there was one stack for the whole core, and every client on a
token had the same name. With one person that is fine. With two it is wrong in
a way nobody can see coming: the person on the phone presses Undo and the
laptop's last move disappears, because it was the last thing on the stack.

## Whose change is it

A token is a permission, not a person. One token is routinely shared by a
laptop, a phone and a tablet at the same show. So the core gives every caller a
client id as well: the token id, a dot, and a name for the connection. Every
`/rpc` socket has one, a browser tab keeps its own across reloads, and an HTTP
caller can name itself in the envelope. Patches carry it as `source_client`,
which is what lets each mirror tell its own echo from somebody else's change,
and the scene server files every step of history under it.

## One stack per client

Each client has its own undo stack, redo stack, `history.mark` group and
transaction. A change goes on the stack of the client that made it. Undo pops
from the asker's stack and nobody else's.

An open transaction belongs to one client too. It used to be a snapshot of the
whole document, and any other client was refused until it closed. Now it is a
list of the client's own patches, published as one at the commit. Other
clients' edits go through and are published straight away. An abort takes
back the client's own patches and leaves anything somebody else touched since.

## Refuse rather than merge

A step on your stack records, for every record it touches, the state you left
it in. When you undo, the core checks that each record is still in that state.
If somebody else has changed it since, putting your old state back would throw
their work away without either of you seeing it happen.

There were two ways to go. One is to merge: undo only the fields that are still
yours and leave theirs. The other is to refuse the whole step, say who is in
the way, and let the person decide. GodwinMix refuses.

Merging sounds kinder, but a record is one item, and a half undone item is a
state neither person asked for. A box with your old position and their new
size is not something anybody laid out. And on a phone, nobody can see which
half they got. A refusal is one sentence ("item 'lower third', changed by
iPhone Safari") and two ways on: `force: true` to put your version back
anyway, or change the item by hand. The step stays on the stack either way, so
nothing is lost by being asked.

The check is on the state, not on who wrote last. Somebody who changed the
item and then undid their own change has left it as you left it, and your undo
goes through.

## Drafts

The composer edits a draft, which is a copy of one scene. The draft records
the scene as it was when it was taken and the document's revision at that
moment. Applying it compares the live scene with that record. If they differ,
somebody changed the scene while the draft was open, and the apply is refused
with the list of what changed. The same two ways on apply: start again from the
scene as it is, or `force: true`.

Comparing the scene itself rather than counting revisions means an edit to any
other scene does not make a draft stale. Only a change to the scene the draft
came from does.

## What it costs

Nothing that touches the programme. History is per client, in memory, kept for
the 64 clients that edited most recently, 200 steps each, and each step is a
diff of the records that changed rather than a copy of the document. Who
changed a record last is a few strings per record. All of it is updated under
the scene server's lock in the same place the patch is computed, and none of it
runs on a streaming thread.
