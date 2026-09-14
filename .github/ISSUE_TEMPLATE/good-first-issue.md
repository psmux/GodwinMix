---
name: Good first issue (maintainers)
about: File a first issue that somebody can actually finish
title: ""
labels: ["good first issue"]
assignees: ""
---

<!--
For maintainers. Fill in all five sections or do not file it as a first issue.

The label on its own does not work. A 2020 study of 9,368 good first issues
across 816 projects found that almost half were never solved by a newcomer:
the label marked the issue as available and then left the person to work out
where the code was, what "done" meant, and whether anybody was listening. What
changes the outcome is the label plus a reply inside 48 hours, and the sections
below are what makes that reply possible for whoever is on triage that day.

Delete these comments as you fill it in.
-->

## What is wrong, or what is missing

<!-- Two or three sentences. Enough that somebody who has never opened this
repository understands the problem, without reading any code. -->

## The file to change

<!-- The path, and the function or the line if you know it. One file where you
can manage it. If it is genuinely two, name both and say which to start with.

  crates/godwinmix/src/cli/observe.rs, in `Filter::keeps()`
-->

## The test to run

<!-- The exact command, and what it does now against what it should do. If the
test does not exist yet, say what it should assert and where it goes; writing
that test is a fine first issue on its own.

  cargo test -p godwinmix the_since_filter_takes_a_time_of_day_or_a_timestamp
-->

## Acceptance, in one line

<!-- One sentence that somebody can check from the outside.

  `gmx logs --since 20:10` keeps a line written at 20:10:00 exactly.
-->

## Who to ask

<!-- A name, and where to ask: this thread, or discussions. Not "the
maintainers". A first issue with nobody named is a first issue that stalls the
first time it is confusing. -->

---

Before you start, say so on this thread and it is yours. You do not need
permission and you do not need to finish: an abandoned attempt with a note
saying where you got stuck is useful, and it is usually a documentation bug.

First response on this thread is targeted at under 48 hours. If three days pass
with nothing, assume it was missed and say so here. What the median actually is
this month is at
[community/dashboard.md](https://github.com/psmux/GodwinMix/blob/main/community/dashboard.md).

Setting up: [CONTRIBUTING.md](https://github.com/psmux/GodwinMix/blob/main/CONTRIBUTING.md).
