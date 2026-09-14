# The friction log

Every time somebody gets stuck on the path from "I want to use this" to "it
works", it goes in this file. Not in a private note, not in a ticket that gets
closed, here, where the next person can read it.

The rule: if you got stuck, you write it down, even if you worked it out
yourself thirty seconds later. Especially then. A thirty second confusion that
happens to everybody costs more in aggregate than an hour long problem that
happens to one person.

That applies to AI agents as well as people. An agent that had to read the
source to find out what a command does has hit friction, and the fix is the
same fix.

## How to add an entry

Open a pull request adding a row, or open an issue with the same information
and somebody will add it. One line is enough. Do not polish it.

```
### 2026-09-14: the token is not in the error

Ran `gmx ctl status` against a mixer with a token set. Got `401 Unauthorized`
and nothing else. Took ten minutes to work out it wanted GODWINMIX_TOKEN,
because the error did not say so.

Fix: the 401 body now names the environment variable and the flag. (#123)
Status: fixed
```

Three things make an entry useful: what you were trying to do, what you saw,
and what you expected instead. The fix and the issue number get added later by
whoever does the work.

## What is done with them

Every entry is triaged. The outcome is one of:

* **Fixed.** The error message, the default, the page or the command changed.
  This is the usual one and it is usually cheap.
* **Documented.** The behaviour is right and the page that should have said so
  now says so.
* **Accepted.** The friction is real and the fix is expensive or is waiting on
  something else. The entry says which, so the next person who hits it knows it
  is known.

An entry is never closed with "works as intended" and nothing else. If three
people trip over the same intention, the intention is the problem.

## The score

The measured time from nothing to a working plugin is the number this project
keeps: under 15 minutes for the Python template, under 30 for Rust, measured by
running it in CI on a clean container. This log is where the things that make
that number worse are recorded.

---

## Entries

### 2026-09-14: no entries yet

This file was created with the documentation, before anybody outside the
project had tried to use it. That is the wrong way round and it is the reason
the file exists: the first real entries will come from the first people who are
not the author, and there is somewhere to put them.

Status: waiting
