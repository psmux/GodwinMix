// The failing test. It is meant to fail until you replace it.
//
// `./check` runs it last, with `go test .`. It fails on a fresh template on
// purpose, because a template that passes out of the box teaches nothing and a
// green tick on an unfinished plugin is a lie.
//
// It sits beside main.go rather than under tests/, because a Go test can only
// reach draw() from inside package main. tests/ holds the two helper programs
// and the transcript, which are separate packages and do not need draw().
//
// Replace the tests with checks of the picture your plugin actually draws. Two
// that are worth writing:
//
//   - the frame is the right size for the canvas (the one below, keep it)
//   - one pixel you can predict has the value you expect (the one below, change it)
//
// Then set deliberatelyFailing to false.

package main

import (
	"bytes"
	"testing"
)

const deliberatelyFailing = true

var testCanvas = canvas{width: 160, height: 90, fps: 30}

func TestFrameIsTheRightSize(t *testing.T) {
	frame := draw(testCanvas, map[string]any{"bars": 8.0}, 0)
	expected := testCanvas.width * testCanvas.height * 3 / 2
	if len(frame) != expected {
		t.Errorf("draw() returned %d bytes, an I420 frame at %dx%d is %d", len(frame), testCanvas.width, testCanvas.height, expected)
	}
}

func TestTheFirstPixelIsWhatYouMeant(t *testing.T) {
	frame := draw(testCanvas, map[string]any{"bars": 8.0}, 0)
	if frame[0] != 235 {
		t.Errorf("the top left luma is %d, not the white bar's 235", frame[0])
	}
}

// Keep this one. A 1080p frame is over 2**21 bytes, so its EBML size needs four
// bytes with the marker bit in bit 4 of the first, and that is the one place a
// hand written Matroska writer usually goes wrong.
func TestVintHoldsA1080pFrame(t *testing.T) {
	if got := vint(1); !bytes.Equal(got, []byte{0x81}) {
		t.Errorf("vint(1) is %x, not 81", got)
	}
	if got := vint(127); !bytes.Equal(got, []byte{0x40, 0x7f}) {
		t.Errorf("vint(127) is %x, not 407f", got)
	}
	// 1920 * 1080 * 3 / 2, plus the four byte SimpleBlock head.
	if got := vint(3110404); !bytes.Equal(got, []byte{0x10, 0x2f, 0x76, 0x04}) {
		t.Errorf("vint(3110404) is %x, not 102f7604", got)
	}
}

func TestThePictureTestHasBeenWritten(t *testing.T) {
	if deliberatelyFailing {
		t.Fatal("picture_test.go has not been written yet. Replace the tests with checks of your own picture, then set deliberatelyFailing to false.")
	}
}
