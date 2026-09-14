#!/usr/bin/env python3
"""The listing bot: what runs on a pull request that touches index.json.

Every rule here is one the Rust reader or the quality scale already states.
`crates/godwinmix-host/src/marketplace.rs` says what the document has to be,
`crates/godwinmix-host/src/sources/mod.rs` says what a source may look like,
and the four tiers say what a listing has to have earned. This file turns
those into checks that print one line each, so an author reading a failed run
sees the name of the check that refused the listing and what it wanted.

It has no dependencies. Python 3.8 and anything later runs it, and no wheel
has to be installed on a runner before the first check can run.

    python3 bot/validate.py                       every entry in index.json
    python3 bot/validate.py --entry ndi           one entry
    python3 bot/validate.py --entry ndi --plugin-dir /tmp/gmx-ndi
    python3 bot/validate.py --self-test           the fixtures in bot/fixtures

The last form is how this file is tested. Each fixture is a whole index
document named for what it should do, and the self test asserts that each one
is accepted or refused with the check its name promises.
"""

import argparse
import json
import os
import re
import subprocess
import sys
from collections import namedtuple

try:  # Python 3.11 and later.
    import tomllib
except ImportError:  # pragma: no cover, older runners take the small reader.
    tomllib = None

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
DEFAULT_INDEX = os.path.join(ROOT, "index.json")
FIXTURES = os.path.join(HERE, "fixtures")

# ---------------------------------------------------------------------------
# The vocabularies, copied from the Rust that enforces them
# ---------------------------------------------------------------------------

# godwinmix_protocol::plugin::manifest::PLATFORMS
PLATFORMS = [
    "linux-x86_64",
    "linux-aarch64",
    "linux-armv7",
    "macos-aarch64",
    "macos-x86_64",
    "windows-x86_64",
    "windows-aarch64",
]

# godwinmix_protocol::plugin::manifest::KINDS
KINDS = [
    "source",
    "output",
    "filter",
    "transition",
    "encoder",
    "service",
    "device",
    "panel",
    "surface",
    "preset",
    "graphic",
    "collection",
]

# marketplace::Tier, lowest first. The order is the order the core sorts by.
TIERS = ["custom", "bronze", "silver", "gold"]

SCHEMA_VERSION = 1

HARNESS_LINE = re.compile(r"^(?P<platform>[A-Za-z0-9_]+-[A-Za-z0-9_]+): (?P<passed>\d+)/(?P<total>\d+)$")
SEMVER = re.compile(r"^\d+\.\d+\.\d+([-+].*)?$")
DATE = re.compile(r"^\d{4}-\d{2}-\d{2}$")

# ---------------------------------------------------------------------------
# Results
# ---------------------------------------------------------------------------

OK = "ok"
FAILED = "FAIL"
SKIPPED = "skip"

Result = namedtuple("Result", "status check detail")


def plural(n, singular, plural_form=None):
    """`1 entry`, `3 entries`. Every detail line reads like a sentence."""
    if n == 1:
        return "1 %s" % singular
    return "%d %s" % (n, plural_form or singular + "s")


class Refused(Exception):
    """A check said no. The message is the detail that gets printed."""


class Skip(Exception):
    """A check had nothing to look at. Not a failure."""


# ---------------------------------------------------------------------------
# What a check is given
# ---------------------------------------------------------------------------


class Ctx:
    """Everything a check may look at: the document, one entry, the options."""

    def __init__(self, doc, entry=None, plugin_dir=None, gmx=None, path=None):
        self.doc = doc
        self.entry = entry
        self.plugin_dir = plugin_dir
        self.gmx = gmx or os.environ.get("GMX", "gmx")
        self.path = path

    def named(self, detail):
        """Put the entry's name in front of a detail line."""
        if self.entry is None:
            return detail
        return "%s: %s" % (self.entry.get("name", "?"), detail)

    @property
    def tier(self):
        return (self.entry or {}).get("tier", "custom")

    def tier_is_at_least(self, tier):
        mine = self.tier
        if mine not in TIERS:
            return False
        return TIERS.index(mine) >= TIERS.index(tier)

    @property
    def versions(self):
        return (self.entry or {}).get("versions", []) or []


# ---------------------------------------------------------------------------
# Reading a source, the way Source::parse reads one
# ---------------------------------------------------------------------------


def looks_like_a_path(spec):
    """Source::parse's own test, character for character."""
    if spec in (".", ".."):
        return True
    if spec.startswith(("./", "../", ".\\", "..\\", "/", "~")):
        return True
    # C:\plugins\clock. One letter before the colon is a drive, not a scheme.
    return len(spec) > 2 and spec[1] == ":" and spec[0].isalpha() and spec[2] in "\\/"


def split_version(spec):
    """`name@1.2.3` into its halves. A leading @ is an npm scope, not a version."""
    at = spec.rfind("@")
    if at > 0:
        return spec[:at], spec[at + 1:]
    return spec, None


def github_from_url(url):
    """github::from_url: the web URL people paste, turned into owner/repo."""
    for prefix in ("https://github.com/", "http://github.com/", "https://www.github.com/"):
        if url.startswith(prefix):
            parts = url[len(prefix):].rstrip("/").split("/")
            if len(parts) >= 2 and parts[0] and parts[1]:
                return "github", "%s/%s" % (parts[0], parts[1][:-4] if parts[1].endswith(".git") else parts[1])
    return None


def parse_source(spec):
    """Answer (form, what it resolves to), or raise Refused.

    The forms are the ones Source::parse accepts. A path parses there and is
    refused here, because a directory on your machine is nothing on anyone
    else's, and an index entry has to resolve for a stranger.
    """
    spec = (spec or "").strip()
    if not spec:
        raise Refused("the source is empty. Write owner/repo, a .git URL, or cargo:, npm:, pypi: or oci:")
    for scheme in ("cargo", "npm", "pypi"):
        if spec.startswith(scheme + ":"):
            package, version = split_version(spec[len(scheme) + 1:])
            if not package:
                raise Refused("`%s` names no package after the scheme" % spec)
            return scheme, package + ("@" + version if version else "")
    if spec.startswith("oci:"):
        if not spec[4:]:
            raise Refused("`%s` names no image reference" % spec)
        return "oci", spec[4:]
    if looks_like_a_path(spec):
        raise Refused(
            "`%s` is a path. A path installs only on the machine it is typed on, so the index "
            "takes owner/repo, a .git URL, or a registry scheme" % spec
        )
    if spec.startswith("git+"):
        return "git", spec[4:].split("#")[0]
    if spec.startswith(("http://", "https://", "ssh://")):
        url = spec.split("#")[0]
        if url.endswith(".git"):
            return "git", url
        found = github_from_url(url)
        if found:
            return found
        raise Refused(
            "`%s` is a URL but not one the core understands. A git source ends in .git; a "
            "GitHub repository is written owner/repo" % spec
        )
    if spec.startswith("git@"):
        return "git", spec.split("#")[0]
    name, version = split_version(spec)
    parts = name.split("/")
    if len(parts) == 2 and parts[0] and parts[1]:
        return "github", name + ("@" + version if version else "")
    raise Refused(
        "`%s` is not a source. Write owner/repo, https://host/x/y.git, cargo:name, "
        "npm:@scope/name, pypi:name or oci:ref" % spec
    )


def is_slug(s):
    """manifest::is_slug, character for character."""
    return bool(
        s
        and len(s) <= 64
        and s[0].isascii()
        and s[0].islower()
        and s[0].isalpha()
        and all(c.isdigit() or c == "-" or (c.isascii() and c.islower() and c.isalpha()) for c in s)
        and not s.endswith("-")
        and "--" not in s
    )


def parse_harness(line):
    """`linux-x86_64: 8/8` into (platform, passed, total), or raise Refused."""
    match = HARNESS_LINE.match(line or "")
    if not match:
        raise Refused(
            "`%s` is not a harness result. The bot writes them as `<platform>: <passed>/<total>`, "
            "for example `linux-x86_64: 8/8`" % line
        )
    return match.group("platform"), int(match.group("passed")), int(match.group("total"))


def passing_platforms(version):
    """The platforms whose harness line says every check passed."""
    out = []
    for line in version.get("harness", []) or []:
        platform, passed, total = parse_harness(line)
        if total > 0 and passed == total:
            out.append(platform)
    return out


# ---------------------------------------------------------------------------
# The document checks
# ---------------------------------------------------------------------------


def check_document_shape(ctx):
    """`name` and `plugins`, which is all Marketplace::parse insists on."""
    if not isinstance(ctx.doc, dict):
        raise Refused("the document is not a JSON object")
    name = ctx.doc.get("name", "")
    if not isinstance(name, str) or not name.strip():
        raise Refused("a marketplace needs a `name`, the slug that names it in `gmx marketplace list`")
    if not is_slug(name):
        raise Refused("`%s` is not a slug, and the name becomes a file name in the cache" % name)
    plugins = ctx.doc.get("plugins")
    if not isinstance(plugins, list):
        raise Refused("`plugins` must be an array, even when it is empty")
    return "%s, %s" % (name, plural(len(plugins), "entry", "entries"))


def check_document_version(ctx):
    """A core refuses a document that declares a version it does not read."""
    version = ctx.doc.get("version", 1)
    if not isinstance(version, int) or isinstance(version, bool):
        raise Refused("`version` must be an integer, and it is %r" % version)
    if version > SCHEMA_VERSION:
        raise Refused(
            "this document declares schema version %d and the core reads version %d"
            % (version, SCHEMA_VERSION)
        )
    return "schema version %d" % version


def check_document_signing(ctx):
    """Without this block nothing verifies the assets this index points at."""
    signing = ctx.doc.get("signing")
    if not isinstance(signing, dict):
        raise Refused(
            "there is no `signing` block, so every plugin from this index installs unverified. "
            "Name the identity_regexp your CI signs with and its oidc_issuer"
        )
    for key in ("identity_regexp", "oidc_issuer"):
        if not signing.get(key):
            raise Refused("the signing block has no `%s`" % key)
    try:
        re.compile(signing["identity_regexp"])
    except re.error as e:
        raise Refused("identity_regexp does not compile: %s" % e)
    return signing["oidc_issuer"]


def check_document_names_are_unique(ctx):
    """Two entries with one name means `gmx plugin add` picks the first."""
    seen = {}
    for entry in ctx.doc.get("plugins", []):
        name = (entry or {}).get("name")
        if name in seen:
            raise Refused("`%s` is listed twice. One entry per plugin, with its versions inside it" % name)
        seen[name] = True
    return "%s, all distinct" % plural(len(seen), "name")


def check_document_cores(ctx):
    """`cores` feeds the dashboard. It is optional, and wrong is worse than absent."""
    cores = ctx.doc.get("cores")
    if cores is None:
        raise Skip("no `cores` block, so compatibility.md will carry no core column")
    if not isinstance(cores, list) or not cores:
        raise Refused("`cores` must be a non empty array of core releases")
    for core in cores:
        if not SEMVER.match(str(core.get("version", ""))):
            raise Refused("a core release has no semver `version`")
        for key in ("api", "compatible"):
            if not isinstance(core.get(key), int) or isinstance(core.get(key), bool):
                raise Refused("core %s has no integer `%s`" % (core.get("version"), key))
        if core["compatible"] > core["api"]:
            raise Refused(
                "core %s says it reads api %d and up to %d, which is backwards"
                % (core["version"], core["compatible"], core["api"])
            )
    return ", ".join(str(c["version"]) for c in cores)


# ---------------------------------------------------------------------------
# The entry checks
# ---------------------------------------------------------------------------


def check_entry_name(ctx):
    """The name is the namespace of every id the plugin contributes."""
    name = ctx.entry.get("name", "")
    if not is_slug(name):
        raise Refused(
            "`%s` is not a slug. Lower case letters, digits and hyphens, starting with a letter, "
            "no trailing hyphen and no doubled hyphen, because the name prefixes every id" % name
        )
    return ctx.named("a slug")


def check_entry_source(ctx):
    """One of the forms Source::parse accepts, and not a path."""
    form, resolved = parse_source(ctx.entry.get("source"))
    return ctx.named("%s, %s" % (resolved, form))


def check_entry_tier(ctx):
    """custom, bronze, silver or gold, and nothing else."""
    tier = ctx.entry.get("tier", "custom")
    if tier not in TIERS:
        raise Refused(
            "`%s` is not a tier. The four are %s, and a listing with nothing checked is custom"
            % (tier, ", ".join(TIERS))
        )
    return ctx.named(tier)


def check_entry_kinds(ctx):
    """The kinds have to be ones the core registers, or nothing finds the plugin."""
    kinds = ctx.entry.get("kinds", []) or []
    if not isinstance(kinds, list):
        raise Refused("`kinds` must be an array")
    for kind in kinds:
        if kind not in KINDS:
            raise Refused(
                "`%s` is not a kind. The core knows %s" % (kind, ", ".join(KINDS))
            )
    return ctx.named(", ".join(kinds) if kinds else "none declared")


def check_entry_metadata(ctx):
    """The three fields a person reads before installing anything."""
    description = ctx.entry.get("description", "")
    if not description.strip():
        raise Refused("write one sentence saying what this plugin does. It is read first, by people and by models")
    if len(description) > 1024:
        raise Refused("the description is %d characters; keep it under 1024" % len(description))
    if not ctx.entry.get("license", "").strip():
        raise Refused("name a licence, an SPDX id such as MIT or Apache-2.0")
    repository = ctx.entry.get("repository", "")
    if repository and not repository.startswith(("https://", "http://")):
        raise Refused("`repository` should be the URL a person opens to read the code")
    for key in ("added", "docs_reviewed"):
        if ctx.entry.get(key) and not DATE.match(ctx.entry[key]):
            raise Refused("`%s` should be a date, written YYYY-MM-DD" % key)
    return ctx.named("%s, %s" % (ctx.entry.get("license"), repository or "no repository"))


def check_entry_versions(ctx):
    """Every version needs a semver and an integer api, newest last."""
    versions = ctx.versions
    if not isinstance(versions, list):
        raise Refused("`versions` must be an array")
    if not versions:
        raise Skip(ctx.named("no versions listed yet"))
    for version in versions:
        label = version.get("version", "?")
        if not SEMVER.match(str(label)):
            raise Refused("`%s` is not semver. Write it MAJOR.MINOR.PATCH" % label)
        api = version.get("api")
        if not isinstance(api, int) or isinstance(api, bool):
            raise Refused(
                "version %s declares no integer `api`. Copy plugin.api out of its "
                "gmx-plugin.toml; the core runs a plugin only when the level is one it reads" % label
            )
        if api < 1:
            raise Refused("version %s declares api %r, and api starts at 1" % (label, api))
        if "signed" in version and not isinstance(version["signed"], bool):
            raise Refused("version %s has a non boolean `signed`" % label)
    return ctx.named("%s, newest %s at api %d" % (plural(len(versions), "version"), versions[-1]["version"], versions[-1]["api"]))


def check_entry_platforms(ctx):
    """Platforms come from the list the core matches release assets against."""
    seen = set()
    for version in ctx.versions:
        platforms = version.get("platforms", []) or []
        if not isinstance(platforms, list):
            raise Refused("version %s has a non array `platforms`" % version.get("version"))
        for platform in platforms:
            if platform not in PLATFORMS:
                raise Refused(
                    "version %s names `%s`, which is not a platform triple. The seven are %s"
                    % (version.get("version"), platform, " ".join(PLATFORMS))
                )
            seen.add(platform)
        if len(set(platforms)) != len(platforms):
            raise Refused("version %s names a platform twice" % version.get("version"))
    if not seen:
        raise Skip(ctx.named("no platforms declared"))
    return ctx.named(" ".join(p for p in PLATFORMS if p in seen))


def check_entry_harness(ctx):
    """Every harness line has to read `<platform>: <passed>/<total>`."""
    lines = 0
    for version in ctx.versions:
        declared = version.get("platforms", []) or []
        for line in version.get("harness", []) or []:
            platform, passed, total = parse_harness(line)
            lines += 1
            if platform not in PLATFORMS:
                raise Refused("`%s` names `%s`, which is not a platform triple" % (line, platform))
            if declared and platform not in declared:
                raise Refused(
                    "version %s has a result for %s, which it does not list in `platforms`"
                    % (version.get("version"), platform)
                )
            if passed > total:
                raise Refused("`%s` passed more checks than it ran" % line)
    if not lines:
        raise Skip(ctx.named("no harness results recorded"))
    return ctx.named("%s, all well formed" % plural(lines, "result"))


def check_entry_bronze(ctx):
    """bronze: the manifest validates, the harness passes on at least one
    platform, a SKILL.md is present, and there is a release with a version."""
    if not ctx.tier_is_at_least("bronze"):
        raise Skip(ctx.named("custom, so nothing is required and nothing is promised"))
    if not ctx.versions:
        raise Refused("bronze needs a release with a version, and this entry lists none")
    skill = ctx.entry.get("skill")
    if ctx.plugin_dir:
        found = os.path.isfile(os.path.join(ctx.plugin_dir, skill or "SKILL.md"))
        if not found:
            raise Refused(
                "bronze needs a SKILL.md, and %s has none. It is what an agent reads to drive "
                "the plugin" % ctx.plugin_dir
            )
    elif not (isinstance(skill, str) and skill.endswith(".md")):
        raise Refused(
            "bronze needs a SKILL.md, and the entry names none. Add `\"skill\": \"SKILL.md\"` "
            "with the path inside your plugin"
        )
    newest = ctx.versions[-1]
    passed = passing_platforms(newest)
    if not passed:
        raise Refused(
            "bronze needs the harness to pass on at least one platform, and version %s has no "
            "passing result. Run `gmx plugin test .` and fix what it names" % newest["version"]
        )
    return ctx.named("version %s passes on %s" % (newest["version"], ", ".join(passed)))


def check_entry_silver(ctx):
    """silver: the harness passes on every platform the listing declares."""
    if not ctx.tier_is_at_least("silver"):
        raise Skip(ctx.named("below silver"))
    for version in ctx.versions:
        declared = version.get("platforms", []) or []
        if not declared:
            raise Refused("silver needs version %s to declare the platforms it ships" % version["version"])
        passed = set(passing_platforms(version))
        missing = [p for p in declared if p not in passed]
        if missing:
            raise Refused(
                "silver needs a passing harness result for every declared platform, and version "
                "%s is missing %s. Either publish for those platforms or drop them from the list"
                % (version["version"], ", ".join(missing))
            )
    return ctx.named("every declared platform passes, on all %s" % plural(len(ctx.versions), "version"))


def check_entry_gold(ctx):
    """gold: two or more maintainers or the project itself, an eval suite with
    recorded shows, documentation reviewed, and a place on the official
    marketplace."""
    if ctx.tier != "gold":
        raise Skip(ctx.named("below gold"))
    maintainers = ctx.entry.get("maintainers", []) or []
    if len(maintainers) < 2 and ctx.entry.get("owner") != ctx.doc.get("owner"):
        raise Refused(
            "gold is maintained by two or more people or by the project. Name them in "
            "`maintainers`, so an operator knows who answers when it breaks mid show"
        )
    if not ctx.entry.get("evals"):
        raise Refused("gold needs an eval suite with recorded shows. Point `evals` at it")
    if not ctx.entry.get("docs_reviewed"):
        raise Refused("gold needs its documentation reviewed. Put the date in `docs_reviewed`")
    if ctx.entry.get("official") is not True:
        raise Refused("gold is on the official marketplace. Set `official` once it is listed there")
    return ctx.named("%s, evals at %s, docs read %s" % (plural(len(maintainers), "maintainer"), ctx.entry["evals"], ctx.entry["docs_reviewed"]))


def check_entry_policy(ctx):
    """The listing policy, made a field so a reviewer has something to read."""
    if not ctx.tier_is_at_least("bronze"):
        raise Skip(ctx.named("custom, and a custom listing promises nothing"))
    policy = ctx.entry.get("policy")
    if not isinstance(policy, dict):
        raise Refused(
            "declare a `policy` with `network`, `secrets` and `telemetry`. An empty array means "
            "none, and leaving the key out is not the same as saying none"
        )
    for key in ("network", "secrets"):
        if not isinstance(policy.get(key), list):
            raise Refused("`policy.%s` must be an array, empty when there is nothing to declare" % key)
    telemetry = policy.get("telemetry")
    if not isinstance(telemetry, str) or not telemetry.strip():
        raise Refused(
            "`policy.telemetry` must say `none`, or say what is collected, where it goes and how "
            "it is switched off"
        )
    return ctx.named(
        "network %d, secrets %d, telemetry %s"
        % (len(policy["network"]), len(policy["secrets"]), telemetry)
    )


def check_entry_manifest(ctx):
    """The checked out source: its manifest agrees with the listing and the
    entry point it names is really there."""
    if not ctx.plugin_dir:
        raise Skip(ctx.named("no --plugin-dir, so nothing was checked out to read"))
    path = os.path.join(ctx.plugin_dir, "gmx-plugin.toml")
    if not os.path.isfile(path):
        raise Refused("there is no gmx-plugin.toml in %s" % ctx.plugin_dir)
    manifest = read_manifest(path)
    plugin = manifest.get("plugin", {})
    if plugin.get("name") != ctx.entry.get("name"):
        raise Refused(
            "the manifest calls this plugin `%s` and the listing calls it `%s`. The name is the "
            "namespace of every id, so the two have to agree" % (plugin.get("name"), ctx.entry.get("name"))
        )
    missing = [path for path in entry_points(manifest) if not os.path.exists(os.path.join(ctx.plugin_dir, path))]
    if missing:
        raise Refused(
            "the manifest's [run] names %s, and %s does not exist. Nothing can start this plugin"
            % (", ".join(missing), missing[0])
        )
    if ctx.versions:
        api = plugin.get("api")
        listed = ctx.versions[-1].get("api")
        if isinstance(api, int) and api != listed:
            raise Refused("the manifest declares api %s and the newest listed version says %s" % (api, listed))
    return ctx.named("gmx-plugin.toml agrees with the listing")


def check_entry_harness_run(ctx):
    """`gmx plugin test <dir>`, and its exit status is the answer."""
    if not ctx.plugin_dir:
        raise Skip(ctx.named("no --plugin-dir, so the harness was not run here"))
    argv = [ctx.gmx, "plugin", "test", ctx.plugin_dir]
    try:
        run = subprocess.run(argv, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    except FileNotFoundError:
        raise Refused(
            "`%s` is not on PATH. The bot runs `gmx plugin test` itself, so set GMX to the "
            "binary or install the core on the runner" % ctx.gmx
        )
    if run.returncode != 0:
        tail = [line for line in run.stdout.decode("utf-8", "replace").splitlines() if line.strip()][-6:]
        raise Refused(
            "`%s` exited %d. What it printed last:\n      %s"
            % (" ".join(argv), run.returncode, "\n      ".join(tail))
        )
    return ctx.named("gmx plugin test passed on %s" % ctx.plugin_dir)


# ---------------------------------------------------------------------------
# The whole list, at a glance
# ---------------------------------------------------------------------------

DOCUMENT = "document"
ENTRY = "entry"

Check = namedtuple("Check", "name scope run summary")

CHECKS = [
    Check("document.shape", DOCUMENT, check_document_shape, "the document has a name and a plugins array"),
    Check("document.version", DOCUMENT, check_document_version, "the schema version is one this core reads"),
    Check("document.signing", DOCUMENT, check_document_signing, "the signing block names an identity and an issuer"),
    Check("document.names", DOCUMENT, check_document_names_are_unique, "no two entries share a name"),
    Check("document.cores", DOCUMENT, check_document_cores, "the core releases the dashboard reports against"),
    Check("entry.name", ENTRY, check_entry_name, "the name is a slug"),
    Check("entry.source", ENTRY, check_entry_source, "the source is one of the forms the core parses"),
    Check("entry.tier", ENTRY, check_entry_tier, "the tier is custom, bronze, silver or gold"),
    Check("entry.kinds", ENTRY, check_entry_kinds, "every kind is one the core registers"),
    Check("entry.metadata", ENTRY, check_entry_metadata, "description, licence and repository are there"),
    Check("entry.versions", ENTRY, check_entry_versions, "every version has a semver and an integer api"),
    Check("entry.platforms", ENTRY, check_entry_platforms, "every platform is one of the seven triples"),
    Check("entry.harness", ENTRY, check_entry_harness, "every harness line reads platform: passed/total"),
    Check("entry.bronze", ENTRY, check_entry_bronze, "bronze: a release, a SKILL.md, one platform passing"),
    Check("entry.silver", ENTRY, check_entry_silver, "silver: every declared platform passing"),
    Check("entry.gold", ENTRY, check_entry_gold, "gold: maintainers, evals, docs reviewed, official"),
    Check("entry.policy", ENTRY, check_entry_policy, "network use, secrets and telemetry are declared"),
    Check("entry.manifest", ENTRY, check_entry_manifest, "the checked out manifest agrees and its entry point exists"),
    Check("entry.harness-run", ENTRY, check_entry_harness_run, "gmx plugin test passes on the checked out source"),
]

DOCUMENT_CHECKS = [c for c in CHECKS if c.scope == DOCUMENT]
ENTRY_CHECKS = [c for c in CHECKS if c.scope == ENTRY]


# ---------------------------------------------------------------------------
# Running them
# ---------------------------------------------------------------------------


def run_check(check, ctx):
    try:
        return Result(OK, check.name, check.run(ctx))
    except Skip as e:
        return Result(SKIPPED, check.name, str(e))
    except Refused as e:
        return Result(FAILED, check.name, ctx.named(str(e)))


def validate(doc, entry_name=None, plugin_dir=None, gmx=None):
    """Every result, in the order the checks are declared."""
    results = []
    ctx = Ctx(doc, plugin_dir=plugin_dir, gmx=gmx)
    for check in DOCUMENT_CHECKS:
        results.append(run_check(check, ctx))
    if any(r.status == FAILED and r.check == "document.shape" for r in results):
        return results
    entries = doc.get("plugins", [])
    if entry_name is not None:
        entries = [e for e in entries if (e or {}).get("name") == entry_name]
        if not entries:
            results.append(Result(FAILED, "entry.name", "there is no entry called `%s` in the document" % entry_name))
            return results
    for entry in entries:
        ectx = Ctx(doc, entry=entry, plugin_dir=plugin_dir, gmx=gmx)
        for check in ENTRY_CHECKS:
            results.append(run_check(check, ectx))
    return results


def report(results, out=sys.stdout):
    width = max(len(r.check) for r in results) if results else 20
    for r in results:
        out.write("%-4s %-*s  %s\n" % (r.status, width, r.check, r.detail))


def failures(results):
    return [r for r in results if r.status == FAILED]


def load(path):
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


# ---------------------------------------------------------------------------
# The small manifest reader
# ---------------------------------------------------------------------------


def read_manifest(path):
    """gmx-plugin.toml as a dict.

    tomllib does it on a modern runner. The fallback reads the handful of keys
    the manifest check looks at, which is enough to catch an entry point that
    is not there.
    """
    if tomllib is not None:
        with open(path, "rb") as f:
            return tomllib.load(f)
    return read_manifest_without_tomllib(path)


def read_manifest_without_tomllib(path):
    out = {}
    table = None
    with open(path, "r", encoding="utf-8") as f:
        for raw in f:
            line = raw.split("#")[0].strip()
            if not line:
                continue
            if line.startswith("[["):
                table = None
                continue
            if line.startswith("["):
                table = out.setdefault(line.strip("[]").strip(), {})
                continue
            if table is None or "=" not in line:
                continue
            key, _, value = line.partition("=")
            table[key.strip().strip('"')] = value.strip().strip('"')
    nested = {}
    for name, body in out.items():
        parts = name.split(".")
        here = nested
        for part in parts[:-1]:
            here = here.setdefault(part, {})
        here[parts[-1]] = body
    return nested


def entry_points(manifest):
    """Every path in [run] that has to exist inside the plugin directory."""
    run = manifest.get("run") or {}
    paths = []
    for key in ("python", "node", "shell"):
        if isinstance(run.get(key), str):
            paths.append(run[key])
    for path in (run.get("bin") or {}).values():
        if isinstance(path, str):
            paths.append(path)
    return paths


# ---------------------------------------------------------------------------
# The self test
# ---------------------------------------------------------------------------

# Each fixture is a whole index document. The second column is the check that
# has to refuse it, or None when the fixture is meant to be accepted.
FIXTURE_EXPECTATIONS = [
    ("good-listing.json", None),
    ("bad-source.json", "entry.source"),
    ("bronze-without-harness.json", "entry.bronze"),
    ("silver-missing-platform.json", "entry.silver"),
    ("unknown-tier.json", "entry.tier"),
]

# A directory rather than a document: a plugin whose manifest parses and whose
# entry point is not there.
DIRECTORY_EXPECTATIONS = [
    ("broken-plugin", "entry.manifest"),
]


def self_test(out=sys.stdout):
    """Run every fixture and assert it behaves as its name says."""
    problems = []
    declared = {name for name, _ in FIXTURE_EXPECTATIONS}
    on_disk = {f for f in os.listdir(FIXTURES) if f.endswith(".json")}
    for extra in sorted(on_disk - declared):
        problems.append("%s is in bot/fixtures and not in FIXTURE_EXPECTATIONS" % extra)
    for missing in sorted(declared - on_disk):
        problems.append("%s is expected and not in bot/fixtures" % missing)

    for name, expected in FIXTURE_EXPECTATIONS:
        path = os.path.join(FIXTURES, name)
        if not os.path.isfile(path):
            continue
        results = validate(load(path))
        failed = failures(results)
        names = [r.check for r in failed]
        if expected is None:
            if failed:
                problems.append("%s should be accepted and %s refused it" % (name, ", ".join(names)))
            out.write("%-4s %-30s accepted, %d checks\n" % (OK if not failed else FAILED, name, len(results)))
        elif expected not in names:
            problems.append(
                "%s should be refused by %s and it was refused by %s"
                % (name, expected, ", ".join(names) if names else "nothing")
            )
            out.write("%-4s %-30s wanted %s, got %s\n" % (FAILED, name, expected, ", ".join(names) or "nothing"))
        else:
            detail = next(r.detail for r in failed if r.check == expected)
            out.write("%-4s %-30s refused by %s: %s\n" % (OK, name, expected, first_line(detail)))

    for name, expected in DIRECTORY_EXPECTATIONS:
        directory = os.path.join(FIXTURES, name)
        doc = load(os.path.join(FIXTURES, "good-listing.json"))
        entry = doc["plugins"][0]["name"]
        results = validate(doc, entry_name=entry, plugin_dir=directory)
        names = [r.check for r in failures(results)]
        if expected not in names:
            problems.append("%s should be refused by %s and it was refused by %s" % (name, expected, ", ".join(names) or "nothing"))
            out.write("%-4s %-30s wanted %s, got %s\n" % (FAILED, name, expected, ", ".join(names) or "nothing"))
        else:
            detail = next(r.detail for r in failures(results) if r.check == expected)
            out.write("%-4s %-30s refused by %s: %s\n" % (OK, name, expected, first_line(detail)))

    out.write("\n")
    if problems:
        for problem in problems:
            out.write("FAIL %s\n" % problem)
        out.write("\nself test failed: %s did not behave as named\n" % plural(len(problems), "fixture"))
        return 1
    out.write("self test passed: %s behaved as named\n" % plural(len(FIXTURE_EXPECTATIONS) + len(DIRECTORY_EXPECTATIONS), "fixture"))
    return 0


def first_line(text):
    return text.splitlines()[0] if text else ""


# ---------------------------------------------------------------------------
# The command line
# ---------------------------------------------------------------------------


def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Validate a GodwinMix plugin index and the listings in it.",
        epilog="Checks: " + "; ".join("%s (%s)" % (c.name, c.summary) for c in CHECKS),
    )
    parser.add_argument("--index", default=DEFAULT_INDEX, help="the index document to read (default: the one beside this bot)")
    parser.add_argument("--entry", help="check one entry by name instead of every entry")
    parser.add_argument("--plugin-dir", help="a checked out plugin source, to read its manifest and run the harness against")
    parser.add_argument("--gmx", default=os.environ.get("GMX", "gmx"), help="the gmx binary (default: $GMX, or gmx)")
    parser.add_argument("--self-test", action="store_true", help="run the fixtures in bot/fixtures and assert each behaves as named")
    args = parser.parse_args(argv)

    if args.self_test:
        return self_test()

    try:
        doc = load(args.index)
    except OSError as e:
        print("FAIL document.shape  %s" % e)
        return 2
    except json.JSONDecodeError as e:
        print("FAIL document.shape  %s is not JSON: %s" % (args.index, e))
        return 2

    results = validate(doc, entry_name=args.entry, plugin_dir=args.plugin_dir, gmx=args.gmx)
    report(results)
    failed = failures(results)
    print("")
    if failed:
        for r in failed:
            print("refused by %s: %s" % (r.check, r.detail))
        print("")
        print("%d of %d checks failed. The listing is not merged until they pass." % (len(failed), len(results)))
        return 1
    skipped = len([r for r in results if r.status == SKIPPED])
    print("accepted: %s, %d skipped, %s in %s" % (plural(len(results), "check"), skipped, plural(len(doc.get("plugins", [])), "entry", "entries"), os.path.basename(args.index)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
