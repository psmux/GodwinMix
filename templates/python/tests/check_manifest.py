#!/usr/bin/env python3
"""A rough check of gmx-plugin.toml, so a typo is caught before the core is.

This is not the whole validator. `gmx plugin test .` runs the real one, which
knows every key, every capability name and every JSON Schema the manifest
points at, and reports each problem with its key path. This checks the handful
of mistakes that are worth catching on a machine with no gmx installed, and it
reads the file line by line so it needs no TOML library.
"""
import os
import re
import sys

REQUIRED_PLUGIN = ["name", "version", "api", "description", "license",
                   "platforms", "placements"]
REQUIRED_PROVIDE = ["kind", "id"]
KINDS = ["source", "output", "filter", "transition", "encoder", "service",
         "device", "panel", "surface", "preset", "graphic", "collection"]
PATH_KEYS = ["settings", "skill", "graphic", "collection", "codecs"]


def read(path):
    """A crude TOML reader: the [plugin] table and every [[provides]] block."""
    plugin, provides, table = {}, [], None
    for raw in open(path, encoding="utf-8"):
        line = raw.split("#")[0].strip()
        if not line:
            continue
        if line == "[plugin]":
            table = plugin
            continue
        if line == "[[provides]]":
            table = {}
            provides.append(table)
            continue
        if line.startswith("["):
            table = None
            continue
        if table is None or "=" not in line:
            continue
        key, value = line.split("=", 1)
        table[key.strip()] = value.strip()
    return plugin, provides


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    path = os.path.join(root, "gmx-plugin.toml")
    if not os.path.exists(path):
        print("FAIL: no gmx-plugin.toml at the plugin root")
        return 1
    plugin, provides = read(path)
    problems = []

    for key in REQUIRED_PLUGIN:
        if key not in plugin:
            problems.append("plugin.%s: missing" % key)
    name = plugin.get("name", "").strip('"')
    if name and not re.match(r"^[a-z][a-z0-9-]*[a-z0-9]$", name):
        problems.append("plugin.name: '%s' is not a slug (lower case, digits, hyphens)" % name)
    if "{{" in name:
        problems.append("plugin.name: the '%s' placeholder was never filled in" % name)
    version = plugin.get("version", "").strip('"')
    if version and not re.match(r"^\d+\.\d+\.\d+", version):
        problems.append("plugin.version: '%s' is not semver" % version)
    if plugin.get("api", "0").strip() not in ("1",):
        problems.append("plugin.api: this core serves api 1")

    if not provides:
        problems.append("provides: a plugin with no [[provides]] block registers nothing")
    for index, provide in enumerate(provides):
        at = "provides[%d]" % index
        for key in REQUIRED_PROVIDE:
            if key not in provide:
                problems.append("%s.%s: missing" % (at, key))
        kind = provide.get("kind", "").strip('"')
        if kind and kind not in KINDS:
            problems.append("%s.kind: '%s' is not a kind. Known: %s"
                            % (at, kind, ", ".join(KINDS)))
        if kind == "source":
            for key in ("media", "transports", "settings"):
                if key not in provide:
                    problems.append("%s.%s: a source must declare this" % (at, key))
        for key in PATH_KEYS:
            if key in provide:
                target = provide[key].strip('"')
                if not os.path.exists(os.path.join(root, target)):
                    problems.append("%s.%s: '%s' does not exist" % (at, key, target))

    if problems:
        print("FAIL: gmx-plugin.toml")
        for p in problems:
            print("  " + p)
        return 1
    print("ok: gmx-plugin.toml, %d provide(s)" % len(provides))
    return 0


if __name__ == "__main__":
    sys.exit(main())
