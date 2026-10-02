# Feed paths and templates

What `select` and `template` take, in `feed.test` and in a binding. The
methods themselves are in [the live data reference](live-data.md).

A feed is read into one JSON document whatever it was written in (JSON as it
is, RSS and Atom as `items[]`, a CSV as `rows[]`; the shapes are in
[the formats table](live-data.md#formats)). A path picks a value out of that
document. A template turns what the path picked into words.

## Paths

| Path | Picks |
|---|---|
| empty, `.` or `$` | the whole document |
| `title` | the key `title` of the top object |
| `match.home` | a key inside a key |
| `items[0]` | the first element of a list |
| `items[-1]` | the last element |
| `items[0].title` | a key of an element |
| `items[]` or `items[*]` | every element, as a list |
| `items[].title` | the key `title` of every element, as a list |
| `items[].tags[]` | every tag of every element, as one flat list |
| `["Home team"]` or `['a.b']` | a key that has a space, a dot or a bracket in it |
| `/match/home` | a JSON pointer (RFC 6901), for a key with a slash in it written `~1` |

A leading `$.` is accepted, so a path copied from a JSONPath tool works when
it uses only these forms. Filters and slices (`[?(...)]`, `[1:3]`) are not
part of it; use `limit` to keep the first few of a list.

After `[]` the rest of the path runs on each element. An element the rest
picks nothing from is left out of the list, so `items[].author` is the
authors of the items that have one. If no element has it, the path picks
nothing.

### When a path picks nothing

The path is refused with where it stopped, what was there, and the keys of
the whole document. From `feed.test` on a score feed, with `match.home_goals`
in place of `match.home_score`:

```json
{
  "code": -32602,
  "message": "`match.home_goals` selects nothing: `match` has no key `home_goals`. It has: away, away_score, home, home_score, minute. The top of the document has: match. Pick a path from data.paths, or call feed.test with no select to see the whole document.",
  "data": {
    "field": "select",
    "stopped_at": "match",
    "keys_there": ["away", "away_score", "home", "home_score", "minute"],
    "top_level_keys": ["match"],
    "paths": ["match.away", "match.away_score", "match.home", "match.home_score", "match.minute"],
    "retryable": false
  }
}
```

`data.paths` is the first forty paths in the document, which is usually the
answer. `feed.binding.add` and `feed.binding.set` refuse a path the same way
when the feed has been read, so a binding that would write nothing is not
made.

An index past the end says how long the list is (`items has 3 elements, so
[5] is past the end`), and a path that goes into a string or a number says
what it found there.

## Templates

A template is words with holes. Each hole is a path, read from what `select`
picked:

```
{home} {home_score} : {away_score} {away}
```

on `{"home": "Leeds", "away": "Hull", "home_score": 2, "away_score": 0}`
gives `Leeds 2 : 0 Hull`.

| In a template | Means |
|---|---|
| `{name}` | the path `name`, read from the picked value |
| `{a.b[0]}` | any path from the table above |
| `{}` or `{.}` | the picked value itself |
| `{{` and `}}` | a brace |

A string goes in as it is. A number goes in as written in the feed (`2`,
`2.5`). `true` and `false` go in as those words. Null is nothing. A list of
values goes in joined with a comma and a space; an object goes in as JSON.

When `select` picks a list, the template is filled once for each element and
the answer is a list of strings. A hole that one element lacks is empty in
that element; a hole that no element has is refused, with the keys the
elements do have.

When `select` picks one value, a hole that picks nothing is refused.

## What a binding writes

In this order:

1. `select` picks a value. Empty picks the whole document.
2. For a list, `limit` keeps the first so many. Absent or 0 keeps them all.
3. `template`, if there is one, turns each element (or the one value) into
   words.
4. For a list, `join`, if there is one, makes it one string with `join`
   between the elements.

Without a template a value is written as it is: a string as a string, a list
as a list. Written to a source param whose schema says it is a string, a
number or `true` is written as its words, and a list of numbers as a list of
words, because a feed says `3` where a text wants `"3"`.

The value is compared with what the binding last wrote, and written only if
it differs.
