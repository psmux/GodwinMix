#!/usr/bin/env python3
"""The failing test. It is meant to fail until you replace it.

`./check` runs it last. It fails on a fresh template on purpose, because a
template that passes out of the box teaches nothing and a green tick on an
unfinished plugin is a lie.

Replace the body with a check of the picture your plugin actually draws. Two
that are worth writing:

  * the frame is the right size for the canvas (the one below, keep it)
  * one pixel you can predict has the value you expect (the one below, change it)

Delete DELIBERATELY_FAILING when you have.
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
import main  # noqa: E402

DELIBERATELY_FAILING = True

CANVAS = {"width": 160, "height": 90, "fps": 30}


def test_frame_is_the_right_size():
    frame = main.draw(CANVAS, {"bars": 8}, 0)
    expected = CANVAS["width"] * CANVAS["height"] * 3 // 2
    assert len(frame) == expected, \
        "draw() returned %d bytes, an I420 frame at %dx%d is %d" \
        % (len(frame), CANVAS["width"], CANVAS["height"], expected)


def test_the_first_pixel_is_what_you_meant():
    frame = main.draw(CANVAS, {"bars": 8}, 0)
    assert frame[0] == 235, "the top left luma is %d, not the white bar's 235" % frame[0]


def main_():
    failures = []
    for name, test in sorted(globals().items()):
        if name.startswith("test_") and callable(test):
            try:
                test()
            except AssertionError as e:
                failures.append("%s: %s" % (name, e))
    if DELIBERATELY_FAILING:
        failures.append(
            "test_picture.py has not been written yet. Replace the tests with "
            "checks of your own picture, then set DELIBERATELY_FAILING = False.")
    if failures:
        print("FAIL: tests/test_picture.py")
        for f in failures:
            print("  " + f)
        return 1
    print("ok: the picture is what you meant")
    return 0


if __name__ == "__main__":
    sys.exit(main_())
