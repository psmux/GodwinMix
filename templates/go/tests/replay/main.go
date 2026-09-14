// Replay a recorded transcript against the plugin, with no core running.
//
// Bytes in, bytes out. It starts the built plugin, writes the `core` lines to
// its stdin in order, and checks that each `plugin` line turns up on stderr, as
// a subset. Lines the transcript does not mention (log notifications, media
// reports) are skipped rather than failed, so adding a log line does not break
// the test.
//
// `gmx plugin test --offline` will do this and more once gmx is installed. This
// program is here so the template's check runs on a bare machine today.
//
//	go run ./tests/replay tests/transcript.jsonl bin/{{name}}

package main

import (
	"bufio"
	"bytes"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"strings"
	"sync"
	"time"
)

const answerWait = 10 * time.Second

const exitWait = 8 * time.Second

// step is one line of the transcript: something the core says, or something the
// plugin has to say back.
type step struct {
	line     int
	fromCore bool
	body     any
}

// lineSink collects whole lines from the plugin's stderr as they arrive, so a
// plugin that says nothing cannot wedge the writer and the deadline is real.
type lineSink struct {
	mu      sync.Mutex
	partial []byte
	lines   []string
}

func (s *lineSink) Write(p []byte) (int, error) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.partial = append(s.partial, p...)
	for {
		at := bytes.IndexByte(s.partial, '\n')
		if at < 0 {
			break
		}
		s.lines = append(s.lines, strings.TrimSpace(string(s.partial[:at])))
		s.partial = s.partial[at+1:]
	}
	return len(p), nil
}

func (s *lineSink) at(index int) (string, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if index < len(s.lines) {
		return s.lines[index], true
	}
	return "", false
}

func (s *lineSink) all() []string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return append([]string{}, s.lines...)
}

// matches reports whether actual holds everything expected asks for. "*"
// matches any value.
func matches(expected any, actual any) bool {
	if text, ok := expected.(string); ok && text == "*" {
		return true
	}
	switch want := expected.(type) {
	case map[string]any:
		got, ok := actual.(map[string]any)
		if !ok {
			return false
		}
		for key, value := range want {
			inner, seen := got[key]
			if !seen || !matches(value, inner) {
				return false
			}
		}
		return true
	case []any:
		got, ok := actual.([]any)
		if !ok || len(got) != len(want) {
			return false
		}
		for i, value := range want {
			if !matches(value, got[i]) {
				return false
			}
		}
		return true
	}
	return expected == actual
}

func readSteps(path string) ([]step, error) {
	file, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer file.Close()
	steps := []step{}
	scanner := bufio.NewScanner(file)
	scanner.Buffer(make([]byte, 0, 65536), 8388608)
	number := 0
	for scanner.Scan() {
		number++
		text := strings.TrimSpace(scanner.Text())
		if text == "" || strings.HasPrefix(text, "#") || strings.HasPrefix(text, "//") {
			continue
		}
		var one map[string]any
		if err := json.Unmarshal([]byte(text), &one); err != nil {
			return nil, fmt.Errorf("line %d: %w", number, err)
		}
		core, hasCore := one["core"]
		plugin, hasPlugin := one["plugin"]
		if len(one) != 1 || (!hasCore && !hasPlugin) {
			return nil, fmt.Errorf("line %d: a step has one key, 'core' or 'plugin'", number)
		}
		if hasCore {
			steps = append(steps, step{line: number, fromCore: true, body: core})
		} else {
			steps = append(steps, step{line: number, fromCore: false, body: plugin})
		}
	}
	return steps, scanner.Err()
}

func replay() int {
	transcript := "tests/transcript.jsonl"
	if len(os.Args) > 1 {
		transcript = os.Args[1]
	}
	binary := "bin/{{name}}"
	if len(os.Args) > 2 {
		binary = os.Args[2]
	}
	steps, err := readSteps(transcript)
	if err != nil {
		fmt.Fprintf(os.Stderr, "FAIL: %v\n", err)
		return 1
	}

	sink := &lineSink{}
	plugin := exec.Command(binary)
	plugin.Stderr = sink
	plugin.Env = append(os.Environ(), "GMX_PLUGIN={{name}}", "GMX_PROVIDE=source", "GMX_INSTANCE=test", "GMX_API_LEVEL=1")
	stdin, err := plugin.StdinPipe()
	if err != nil {
		fmt.Fprintf(os.Stderr, "FAIL: %v\n", err)
		return 1
	}
	if err := plugin.Start(); err != nil {
		fmt.Fprintf(os.Stderr, "FAIL: %v. Run ./check, which builds it first.\n", err)
		return 1
	}

	readTo := 0
	failures := []string{}
	for _, one := range steps {
		if one.fromCore {
			data, err := json.Marshal(one.body)
			if err != nil {
				failures = append(failures, fmt.Sprintf("line %d: %v", one.line, err))
				break
			}
			if _, err := stdin.Write(append(data, '\n')); err != nil {
				failures = append(failures, fmt.Sprintf("line %d: the plugin closed stdin early", one.line))
				break
			}
			continue
		}
		deadline := time.Now().Add(answerWait)
		found := false
		for !found && time.Now().Before(deadline) {
			raw, ok := sink.at(readTo)
			if !ok {
				time.Sleep(20 * time.Millisecond)
				continue
			}
			readTo++
			if raw == "" {
				continue
			}
			var actual any
			if err := json.Unmarshal([]byte(raw), &actual); err != nil {
				continue
			}
			if matches(one.body, actual) {
				found = true
			}
		}
		if !found {
			want, _ := json.Marshal(one.body)
			failures = append(failures, fmt.Sprintf("line %d: never saw a line matching %s", one.line, want))
			break
		}
	}

	stdin.Close()
	code := 0
	waited := make(chan error, 1)
	go func() {
		waited <- plugin.Wait()
	}()
	select {
	case err := <-waited:
		if exit, ok := err.(*exec.ExitError); ok {
			code = exit.ExitCode()
		} else if err != nil {
			failures = append(failures, fmt.Sprintf("waiting for the plugin: %v", err))
		}
	case <-time.After(exitWait):
		plugin.Process.Kill()
		failures = append(failures, "the plugin did not exit within 8 seconds of shutdown")
	}

	if len(failures) > 0 {
		fmt.Fprintln(os.Stderr, "FAIL: offline transcript")
		for _, text := range failures {
			fmt.Fprintf(os.Stderr, "  %s\n", text)
		}
		fmt.Fprintln(os.Stderr, "  what the plugin actually said:")
		shown := sink.all()
		if len(shown) > 40 {
			shown = shown[:40]
		}
		for _, raw := range shown {
			if len(raw) > 200 {
				raw = raw[:200]
			}
			fmt.Fprintf(os.Stderr, "    %s\n", raw)
		}
		return 1
	}
	fmt.Fprintf(os.Stderr, "ok: transcript replayed, %d steps, plugin exited %d\n", len(steps), code)
	return 0
}

func main() {
	os.Exit(replay())
}
