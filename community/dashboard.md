# The four numbers

This project measures four things about itself. The list is short on purpose:
a metric that nobody acts on is a metric that teaches people to ignore the
dashboard.

Two of them are counted automatically every six hours by
[`.github/workflows/first-response.yml`](../.github/workflows/first-response.yml)
and land in the block below. The other two are counted by hand once a quarter,
because they need a judgement that an API call cannot make.

## 1. Time to first response

How long somebody waits before anybody replies to their issue or their pull
request. The target is a median under 48 hours over the last 30 days.

This is the first metric because of what it predicts. The MSR 2023 study of
111,094 pull requests found that the wait for a first reply is what decides
whether a newcomer comes back. Not whether the change was merged, not how good
the review was, not how welcoming the README is. Whether somebody answered, and
how fast.

A first response can be a question, or "this needs a week", or "we are not
going to do this, and here is why". Any of those is a response. Silence is the
only failure.

Anything open for more than 48 hours with no reply from anybody but the author
carries the `awaiting-first-response` label, so the queue is one search rather
than a scroll.

## 2. New contributors a month

People whose first merged pull request landed in the last 30 days. Counted from
merged pull requests, so an author who has been around for years but never
merged anything until this month counts once, in the month they merged.

The number moving up is good. The number sitting at zero for a quarter is the
signal that the contribution path has a wall in it somewhere, and the friction
log is where to look for the wall.

## 3. Contributor retention

Of the people who made their first contribution three to six months ago, how
many have contributed since. Counted by hand each quarter, because "contributed
since" includes reviewing somebody else's change and answering a question in
discussions, and no query reads those the way a person does.

A project that gains ten first time contributors a month and keeps none of them
is not growing. It is running a tutorial.

## 4. Bus factor

How many people would have to disappear before a part of the system has nobody
who understands it. Counted by hand each quarter against the areas in
`.github/CODEOWNERS`, and it is currently 1 for most of them, which is the
honest answer for a project this young and the reason review and documentation
matter more here than features do.

## Why stars are not on this list

A star costs nothing, means nothing in particular, and cannot be acted on. A
repository can gain two thousand of them in a week and have the same one person
answering issues on Friday as it had on Monday. Worse, chasing stars changes
behaviour: it rewards the announcement over the reply, and the reply is the
thing that actually decides whether somebody stays.

Downloads and forks have the same problem in a milder form. They are recorded
where GitHub records them and they are not argued about here.

## The numbers

<!-- metrics:start -->

| Metric | Now | Target |
|---|---|---|
| Median time to first response (30 days) | no data | under 48 h |
| Items opened in the window | 0 | |
| Of those, still with no reply | 0 | 0 |
| Open items waiting longer than 48 h | 0 | 0 |
| New contributors (first merged PR, 30 days) | 0 | |

_Last updated never, placeholders until the workflow first runs._

<!-- metrics:end -->

The block above is written by the workflow and committed only when one of the
numbers changes, so the file's history is a record of the project moving rather
than of the schedule firing. The date says when a number last moved, not when
the workflow last ran. The run summary always has the current figures whether
anything moved or not:
<https://github.com/psmux/GodwinMix/actions/workflows/first-response.yml>

Retention and bus factor are written in by hand, in this file, with the date
they were counted.
