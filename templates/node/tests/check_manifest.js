#!/usr/bin/env node
/* A rough check of gmx-plugin.toml, so a typo is caught before the core is.
 *
 * This is not the whole validator. `gmx plugin test .` runs the real one, which
 * knows every key, every capability name and every JSON Schema the manifest
 * points at, and reports each problem with its key path. This checks the handful
 * of mistakes that are worth catching on a machine with no gmx installed, and it
 * reads the file line by line so it needs no TOML library.
 *
 * Everything this script prints goes to stderr, because in this repository
 * stdout belongs to the media stream and nothing else.
 */
"use strict";

const fs = require("node:fs");
const path = require("node:path");

const REQUIRED_PLUGIN = ["name", "version", "api", "description", "license",
  "platforms", "placements"];
const REQUIRED_PROVIDE = ["kind", "id"];
const KINDS = ["source", "output", "filter", "transition", "encoder", "service",
  "device", "panel", "surface", "preset", "graphic", "collection"];
const PATH_KEYS = ["settings", "skill", "graphic", "collection", "codecs"];

/** A crude TOML reader: [plugin], [run], and every [[provides]] block. */
function read(file) {
  const plugin = {};
  const run = {};
  const provides = [];
  let table = null;
  for (const raw of fs.readFileSync(file, "utf8").split("\n")) {
    const line = raw.split("#")[0].trim();
    if (!line) continue;
    if (line === "[plugin]") {
      table = plugin;
      continue;
    }
    if (line === "[run]") {
      table = run;
      continue;
    }
    if (line === "[[provides]]") {
      table = {};
      provides.push(table);
      continue;
    }
    if (line.startsWith("[")) {
      table = null;
      continue;
    }
    if (table === null || !line.includes("=")) continue;
    const at = line.indexOf("=");
    table[line.slice(0, at).trim()] = line.slice(at + 1).trim();
  }
  return { plugin, run, provides };
}

/** The value without its quotes, which is all this reader needs. */
function bare(value) {
  return (value === undefined ? "" : value).replace(/^"+|"+$/g, "");
}

function main() {
  const root = path.dirname(path.dirname(path.resolve(__filename)));
  const file = path.join(root, "gmx-plugin.toml");
  if (!fs.existsSync(file)) {
    console.error("FAIL: no gmx-plugin.toml at the plugin root");
    return 1;
  }
  const { plugin, run, provides } = read(file);
  const problems = [];

  for (const key of REQUIRED_PLUGIN) {
    if (!(key in plugin)) problems.push("plugin." + key + ": missing");
  }
  const name = bare(plugin.name);
  if (name && !/^[a-z][a-z0-9-]*[a-z0-9]$/.test(name)) {
    problems.push("plugin.name: '" + name
      + "' is not a slug (lower case, digits, hyphens)");
  }
  if (name.includes("{{")) {
    problems.push("plugin.name: the '" + name + "' placeholder was never filled in");
  }
  const version = bare(plugin.version);
  if (version && !/^\d+\.\d+\.\d+/.test(version)) {
    problems.push("plugin.version: '" + version + "' is not semver");
  }
  if ((plugin.api === undefined ? "0" : plugin.api).trim() !== "1") {
    problems.push("plugin.api: this core serves api 1");
  }

  // One runtime key, and for this template it is the Node one.
  const runtimes = Object.keys(run);
  if (runtimes.length !== 1) {
    problems.push("run: set exactly one of bin, python, node or shell. This has "
      + runtimes.length);
  } else if (runtimes[0] !== "node") {
    problems.push("run." + runtimes[0] + ": this template is started by Node. "
      + "Use node = \"main.js\", or rewrite the plugin for that runtime.");
  } else if (!fs.existsSync(path.join(root, bare(run.node)))) {
    problems.push("run.node: '" + bare(run.node) + "' does not exist");
  }

  if (provides.length === 0) {
    problems.push("provides: a plugin with no [[provides]] block registers nothing");
  }
  provides.forEach((provide, index) => {
    const at = "provides[" + index + "]";
    for (const key of REQUIRED_PROVIDE) {
      if (!(key in provide)) problems.push(at + "." + key + ": missing");
    }
    const kind = bare(provide.kind);
    if (kind && !KINDS.includes(kind)) {
      problems.push(at + ".kind: '" + kind + "' is not a kind. Known: "
        + KINDS.join(", "));
    }
    if (kind === "source") {
      for (const key of ["media", "transports", "settings"]) {
        if (!(key in provide)) {
          problems.push(at + "." + key + ": a source must declare this");
        }
      }
    }
    for (const key of PATH_KEYS) {
      if (key in provide) {
        const target = bare(provide[key]);
        if (!fs.existsSync(path.join(root, target))) {
          problems.push(at + "." + key + ": '" + target + "' does not exist");
        }
      }
    }
  });

  if (problems.length > 0) {
    console.error("FAIL: gmx-plugin.toml");
    for (const p of problems) console.error("  " + p);
    return 1;
  }
  console.error("ok: gmx-plugin.toml, " + provides.length + " provide(s)");
  return 0;
}

process.exitCode = main();
