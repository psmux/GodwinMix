"""The scripted driver: an operator made of rules, so the harness is tested.

This is what CI runs. It reads the instruction, works out what to call, and
calls it. No model, no network, no cost, and the same answer every time, which
is what makes a failure here a failure of the harness or of the core rather
than a bad afternoon for a language model.

It is deliberately written against the *instruction* and not against the
case's expectations. A driver that read `expect_changes` would make every case
pass and measure nothing.

What it understands is what an operator says at a vision desk: put a camera on
air, cut to black, go back, mute something, turn something down, add or drop a
camera, roll a break. What it does not understand, it does not act on, which is
also how it passes the abstain cases.
"""

import re

# Words that mean "do not touch the mixer": a question, or an instruction so
# vague that guessing is worse than asking.
QUESTIONS = ("is ", "are ", "what", "which", "who", "how", "why", "does ", "can ")
VAGUE = ("look better", "look nicer", "sort it out", "make it good", "tidy up", "improve")


import threading
import time


def drive(core, case, options):
    """One instruction, and the interruption if the case has one.

    With no interruption this is a straight run. With one, the planned calls
    are made with a pause between them and a timer drops whatever is left when
    the interruption lands, which is what makes it an interruption rather than
    a second instruction after the first finished.
    """
    calls = []
    interrupt = case.get("interrupt")
    if not interrupt:
        plan(core, case["instruction"], calls)
        return {"tool_calls": calls}

    stop = threading.Event()
    timer = threading.Timer(interrupt.get("after_ms", 300) / 1000, stop.set)
    timer.start()
    plan(core, case["instruction"], calls, stop=stop, pause=1.0)
    timer.cancel()
    # The first instruction may have finished before the interruption was due.
    # It still arrives; there is just nothing left of the first one to drop.
    stop.set()
    plan(core, interrupt["instruction"], calls)
    return {"tool_calls": calls}


def plan(core, instruction, calls, stop=None, pause=0.0):
    """Work out the calls one instruction asks for, and make them.

    An instruction with a `then` in it is two instructions, and the second
    often leaves the verb out ("the floor camera, then the wide one"), so a
    clause with no verb of its own inherits the one before it.
    """
    clauses = split(instruction)
    carried = None
    for clause in clauses:
        carried = act_on(core, clause, calls, carried, stop, pause)
    return calls


def split(instruction):
    out = []
    for part in re.split(r",?\s+(?:and\s+)?then\s+|,\s+and\s+", instruction.lower()):
        part = part.strip(" .,")
        if part:
            out.append(part)
    return out


def act_on(core, text, calls, carried, stop, pause):
    """One clause. Returns the verb it used, for the clause after it."""
    if carried and not any(rule(text) for rule in (is_take, is_other_verb)):
        # "then the wide one": the verb is the one before it.
        text = f"{carried} {text}"
    sources = {s["id"]: s for s in core.get("sources")}
    status = core.get("core/status")

    for act in (
        abstain,
        revert,
        slate,
        ad_start,
        ad_end,
        add_source,
        remove_source,
        mute,
        gain,
        take,
    ):
        decided = act(core, text, sources, status)
        if decided is None:
            continue
        for method, params in decided:
            if stop is not None and stop.is_set():
                # Something else has arrived. Whatever is left of this
                # instruction is not done.
                return None
            try:
                core.call(method, params)
                calls.append({"method": method, "params": params})
            except Exception as e:
                # A refusal is a result, not a crash: several cases are about
                # the core saying no, and the right behaviour is to stop.
                calls.append({"method": method, "params": params, "refused": str(e)})
                return None
            if pause:
                time.sleep(pause)
        return "take" if act is take else None
    return None


# ---------------------------------------------------------------------------
# The rules, in the order they are tried
# ---------------------------------------------------------------------------


def abstain(core, text, sources, status):
    """The cases where the right answer is to do nothing."""
    if text.rstrip().endswith("?") or text.startswith(QUESTIONS):
        return []
    if any(word in text for word in VAGUE):
        return []
    # An instruction naming something that is not here. Acting on a guess is
    # how an agent puts the wrong camera on air. Not for an add: the whole
    # point of adding a camera is that it is not here yet.
    named = re.findall(r"\b(cam-[a-z0-9-]+|cam[0-9]+)\b", text)
    if named and not is_add(text) and not any(name in sources for name in named):
        return []
    # Already the case. Kubernetes' dry run answers `would_change: false` for
    # exactly this, and the honest answer is no call at all. Only when the
    # instruction actually named the source: a guess that happens to land on
    # what is already up would abstain from an instruction about something
    # else entirely.
    if is_take(text) and not is_add(text):
        wanted = resolve(text, sources)
        if wanted and status.get("program") == wanted:
            return []
    if ("end" in text or "stop" in text or "cut" in text) and "break" in text and not status.get("ad"):
        return []
    return None


def revert(core, text, sources, status):
    if any(
        phrase in text
        for phrase in ("go back", "undo", "revert", "that was wrong", "put it back")
    ):
        return [("program.revert", {})]
    return None


def slate(core, text, sources, status):
    if is_take(text) and ("slate" in text or "black" in text or "nothing" in text):
        return [("program.take", {})]
    return None


def ad_start(core, text, sources, status):
    if ("roll" in text or "start" in text or "play" in text) and ("break" in text or "advert" in text or " ad" in text):
        from run import clip

        return [("adbreak.start", {"uri": clip()})]
    return None


def ad_end(core, text, sources, status):
    if ("end" in text or "stop" in text or "cut" in text or "drop" in text) and (
        "break" in text or "advert" in text or " ad" in text
    ):
        return [("adbreak.end", {})]
    return None


def is_add(text):
    return text.startswith("add") or "add a" in text


def add_source(core, text, sources, status):
    if not is_add(text):
        return None
    match = re.search(r"called ([a-z0-9-]+)", text)
    if not match:
        return []
    new = match.group(1)
    pattern = "test://ball" if "ball" in text else "test://smpte"
    calls = [("source.add", {"id": new, "uri": pattern})]
    if "on air" in text or "on programme" in text or "and take it" in text:
        calls.append(("program.take", {"source": new}))
    return calls


def remove_source(core, text, sources, status):
    # "take cam-floor out" is a removal, not a take, and the two words are at
    # opposite ends of the sentence.
    taken_out = re.search(r"\btake\b.*\bout\b", text)
    if not taken_out and not any(
        word in text for word in ("remove", "drop", "get rid of")
    ):
        return None
    if "break" in text:
        return None
    # "the camera that is on air" names it by what it is doing.
    target = status.get("program") if "on air" in text else None
    target = target or resolve(text, sources)
    if not target:
        return []
    return [("source.remove", {"id": target})]


def mute(core, text, sources, status):
    if "mute" not in text:
        return None
    target = resolve(text, sources)
    if not target:
        return []
    return [("source.audio.set", {"id": target, "muted": "unmute" not in text})]


def gain(core, text, sources, status):
    match = re.search(r"(-?\d+(?:\.\d+)?)\s*db", text)
    if not match:
        return None
    target = resolve(text, sources)
    if not target:
        return []
    db = float(match.group(1))
    # "down to -6" and "down 6" are the same desk, so a bare number with a
    # direction word takes its sign from the direction.
    if db > 0 and ("down" in text or "quieter" in text) and "to" not in text:
        db = -db
    return [("source.audio.set", {"id": target, "gain": 10 ** (db / 20)})]


def take(core, text, sources, status):
    if not is_take(text):
        return None
    target = resolve(text, sources)
    if not target:
        return []
    params = {"source": target}
    seconds = re.search(r"in (\d+) seconds?", text)
    if seconds:
        at = int(status.get("running_time_ms", 0)) + int(seconds.group(1)) * 1000
        params["at_running_time_ms"] = at
    return [("program.take", params)]


# ---------------------------------------------------------------------------
# Reading the instruction
# ---------------------------------------------------------------------------


def is_other_verb(text):
    """A verb that is not a take, so a clause does not inherit one wrongly."""
    return any(
        word in text
        for word in ("mute", "add ", "remove", "drop", "take out", "break", "advert", "db")
    )


def is_take(text):
    if re.search(r"\btake\b.*\bout\b", text):
        return False
    if re.search(r"\bput\b.*\bup\b", text):
        return True
    return any(
        phrase in text
        for phrase in (
            "on air", "on programme", "on program", "switch to", "cut to",
            "take ", "go to", "show ",
        )
    )


def resolve(text, sources):
    """Which source an instruction is talking about.

    Ids are legible slugs (principle five), so the words in `cam-wide` are the
    words a person says. That is the whole matcher: no fuzzy distance, no
    synonyms table, just the id's own words.
    """
    for source_id in sources:
        if source_id in text:
            return source_id
    # The id's own words, and the words in what it is showing. An operator
    # says "the ball camera" about a source whose uri is test://ball, and the
    # uri is in the listing an agent already has.
    words = {source_id: set(words_of(source_id, source)) for source_id, source in sources.items()}
    # A word every source has distinguishes none of them. Without this, "put
    # the floor camera up" matches `cam-wide` on the word `cam`.
    common = set.intersection(*words.values()) if words else set()
    best = None
    best_score = 0
    for source_id, own in words.items():
        score = sum(1 for w in own - common if re.search(rf"\b{re.escape(w)}", text))
        if score > best_score:
            best, best_score = source_id, score
    return best


def words_of(source_id, source):
    out = [w for w in re.split(r"[-_]", source_id) if len(w) > 2]
    out += [
        w
        for w in re.split(r"[^a-z0-9]+", (source.get("uri") or "").lower())
        if len(w) > 2
    ]
    return out
