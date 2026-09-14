// A rough check of gmx-plugin.toml, so a typo is caught before the core is.
//
// This is not the whole validator. `gmx plugin test .` runs the real one, which
// knows every key, every capability name and every JSON Schema the manifest
// points at, and reports each problem with its key path. This checks the
// handful of mistakes that are worth catching on a machine with no gmx
// installed, and it reads the file line by line so it needs no TOML library.

package main

import (
	"fmt"
	"os"
	"regexp"
	"strings"
)

var requiredPlugin = []string{"name", "version", "api", "description", "license", "platforms", "placements"}

var requiredProvide = []string{"kind", "id"}

var kinds = []string{"source", "output", "filter", "transition", "encoder", "service", "device", "panel", "surface", "preset", "graphic", "collection"}

var pathKeys = []string{"settings", "skill", "graphic", "collection", "codecs"}

var slug = regexp.MustCompile(`^[a-z][a-z0-9-]*[a-z0-9]$`)

var semver = regexp.MustCompile(`^\d+\.\d+\.\d+`)

// manifest is what a crude reader can get out of the file: three flat tables
// and a list of provides.
type manifest struct {
	plugin   map[string]string
	run      map[string]string
	build    map[string]string
	provides []map[string]string
}

func read(path string) (manifest, error) {
	m := manifest{}
	m.plugin = map[string]string{}
	m.run = map[string]string{}
	m.build = map[string]string{}
	data, err := os.ReadFile(path)
	if err != nil {
		return m, err
	}
	var table map[string]string
	for _, raw := range strings.Split(string(data), "\n") {
		line := strings.TrimSpace(strings.SplitN(raw, "#", 2)[0])
		if line == "" {
			continue
		}
		if line == "[plugin]" {
			table = m.plugin
			continue
		}
		if line == "[run]" {
			table = m.run
			continue
		}
		if line == "[build]" {
			table = m.build
			continue
		}
		if line == "[[provides]]" {
			table = map[string]string{}
			m.provides = append(m.provides, table)
			continue
		}
		if strings.HasPrefix(line, "[") {
			table = nil
			continue
		}
		if table == nil || !strings.Contains(line, "=") {
			continue
		}
		parts := strings.SplitN(line, "=", 2)
		table[strings.TrimSpace(parts[0])] = strings.TrimSpace(parts[1])
	}
	return m, nil
}

// bare drops the quotes a crude reader leaves on a TOML value.
func bare(value string) string {
	return strings.Trim(strings.TrimSpace(value), `"`)
}

// contains reports whether a list holds a string.
func contains(list []string, want string) bool {
	for _, item := range list {
		if item == want {
			return true
		}
	}
	return false
}

func check() int {
	path := "gmx-plugin.toml"
	if len(os.Args) > 1 {
		path = os.Args[1]
	}
	m, err := read(path)
	if err != nil {
		fmt.Fprintln(os.Stderr, "FAIL: no gmx-plugin.toml at the plugin root")
		return 1
	}
	problems := []string{}
	add := func(text string) {
		problems = append(problems, text)
	}

	for _, key := range requiredPlugin {
		if _, ok := m.plugin[key]; !ok {
			add(fmt.Sprintf("plugin.%s: missing", key))
		}
	}
	name := bare(m.plugin["name"])
	if name != "" && !slug.MatchString(name) {
		add(fmt.Sprintf("plugin.name: '%s' is not a slug (lower case, digits, hyphens)", name))
	}
	if strings.Contains(name, "{{") {
		add(fmt.Sprintf("plugin.name: the '%s' placeholder was never filled in", name))
	}
	version := bare(m.plugin["version"])
	if version != "" && !semver.MatchString(version) {
		add(fmt.Sprintf("plugin.version: '%s' is not semver", version))
	}
	if strings.TrimSpace(m.plugin["api"]) != "1" {
		add("plugin.api: this core serves api 1")
	}

	// One runtime key, and for a compiled plugin it is the bin table. [build]
	// is what lets gmx build the binary from source on a machine that has Go.
	if len(m.run) != 1 {
		add(fmt.Sprintf("run: set exactly one of bin, python, node or shell. This has %d", len(m.run)))
	} else if _, ok := m.run["bin"]; !ok {
		add("run: a Go plugin ships a binary. Use a [run] bin table naming one per platform.")
	}
	for _, key := range []string{"command", "output"} {
		if _, ok := m.build[key]; !ok {
			add(fmt.Sprintf("build.%s: missing. Without it gmx cannot build this plugin from source.", key))
		}
	}
	if strings.Contains(m.run["bin"], "{{") || strings.Contains(m.build["output"], "{{") {
		add("run.bin or build.output: a path still holds a template placeholder")
	}

	if len(m.provides) == 0 {
		add("provides: a plugin with no [[provides]] block registers nothing")
	}
	for index, provide := range m.provides {
		at := fmt.Sprintf("provides[%d]", index)
		for _, key := range requiredProvide {
			if _, ok := provide[key]; !ok {
				add(fmt.Sprintf("%s.%s: missing", at, key))
			}
		}
		kind := bare(provide["kind"])
		if kind != "" && !contains(kinds, kind) {
			add(fmt.Sprintf("%s.kind: '%s' is not a kind. Known: %s", at, kind, strings.Join(kinds, ", ")))
		}
		if kind == "source" {
			for _, key := range []string{"media", "transports", "settings"} {
				if _, ok := provide[key]; !ok {
					add(fmt.Sprintf("%s.%s: a source must declare this", at, key))
				}
			}
		}
		for _, key := range pathKeys {
			value, ok := provide[key]
			if !ok {
				continue
			}
			target := bare(value)
			if _, err := os.Stat(target); err != nil {
				add(fmt.Sprintf("%s.%s: '%s' does not exist", at, key, target))
			}
		}
	}

	if len(problems) > 0 {
		fmt.Fprintln(os.Stderr, "FAIL: gmx-plugin.toml")
		for _, text := range problems {
			fmt.Fprintf(os.Stderr, "  %s\n", text)
		}
		return 1
	}
	fmt.Fprintf(os.Stderr, "ok: gmx-plugin.toml, %d provide(s)\n", len(m.provides))
	return 0
}

func main() {
	os.Exit(check())
}
