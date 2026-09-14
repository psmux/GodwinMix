#!/usr/bin/env python3
"""Write godwinmix-marketplace.json from the manifests under plugins/.

The official marketplace lists the first party plugins and nothing else. It is
generated rather than hand written so that a plugin added under plugins/ cannot
be forgotten here, and so that the version in the listing is the version in the
manifest.

Run from the repository root:  python3 tools/marketplace.py
"""
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "godwinmix-marketplace.json"
REPO = "psmux/godwinmix"


def read_manifest(path):
    """The handful of manifest fields a listing needs, without a TOML parser.

    Python 3.11 has tomllib and 3.9 does not, and this runs in CI on whatever
    is there. The fields wanted are all simple scalars and one array of
    tables, so a few regular expressions are honest here in a way they would
    not be for a general parser.
    """
    text = path.read_text()
    head = text.split("[[provides]]")[0]

    def field(name, default=""):
        m = re.search(rf'^{name}\s*=\s*"([^"]*)"', head, re.M)
        return m.group(1) if m else default

    def number(name, default=0):
        m = re.search(rf"^{name}\s*=\s*(\d+)", head, re.M)
        return int(m.group(1)) if m else default

    def array(name):
        m = re.search(rf"^{name}\s*=\s*\[([^\]]*)\]", head, re.M)
        return re.findall(r'"([^"]*)"', m.group(1)) if m else []

    kinds = re.findall(r'^kind\s*=\s*"([^"]*)"', text, re.M)
    return {
        "name": field("name"),
        "version": field("version"),
        "api": number("api", 1),
        "description": field("description"),
        "license": field("license"),
        "platforms": array("platforms"),
        "kinds": sorted(set(kinds)),
    }


def listing(manifest, directory):
    return {
        "name": manifest["name"],
        "source": f"./plugins/{directory}",
        "description": manifest["description"],
        "tier": "gold",
        "kinds": manifest["kinds"],
        "license": manifest["license"],
        "repository": f"https://github.com/{REPO}/tree/main/plugins/{directory}",
        "versions": [
            {
                "version": manifest["version"],
                "api": manifest["api"],
                "platforms": manifest["platforms"],
                "signed": True,
            }
        ],
    }


def self_test():
    """Check the manifest reader against a manifest with every field in it."""
    import tempfile

    sample = """[plugin]
name = "ndi"
version = "1.2.0"
api = 1
description = "NDI sources and outputs."
license = "Apache-2.0"
platforms = ["linux-x86_64", "macos-aarch64"]
placements = ["sidecar", "node"]

[run]
bin = { "linux-x86_64" = "bin/gmx-ndi" }

[[provides]]
kind = "source"
id = "source"

[[provides]]
kind = "output"
id = "output"
"""
    with tempfile.TemporaryDirectory() as tmp:
        path = pathlib.Path(tmp) / "gmx-plugin.toml"
        path.write_text(sample)
        got = read_manifest(path)
    wanted = {
        "name": "ndi",
        "version": "1.2.0",
        "api": 1,
        "description": "NDI sources and outputs.",
        "license": "Apache-2.0",
        "platforms": ["linux-x86_64", "macos-aarch64"],
        "kinds": ["output", "source"],
    }
    assert got == wanted, f"read {got}, wanted {wanted}"
    entry = listing(got, "ndi")
    assert entry["source"] == "./plugins/ndi", entry
    assert entry["versions"][0]["platforms"] == wanted["platforms"], entry
    print("ok   the manifest reader takes every field a listing needs")
    print("ok   a listing points at the directory under plugins/")


def main():
    if "--self-test" in sys.argv:
        return self_test()
    plugins = []
    for child in sorted((ROOT / "plugins").iterdir()):
        manifest_path = child / "gmx-plugin.toml"
        if not manifest_path.is_file():
            continue
        manifest = read_manifest(manifest_path)
        if not manifest["name"]:
            print(f"skipping {child.name}: its manifest has no name", file=sys.stderr)
            continue
        plugins.append(listing(manifest, child.name))

    document = {
        "name": "godwinmix",
        "title": "GodwinMix official",
        "description": (
            "The plugins the project maintains. Everything here is built on the "
            "public sidecar contract, tested by the conformance harness in CI on "
            "every declared platform, and signed by the release workflow."
        ),
        "owner": REPO.split("/")[0],
        "version": 1,
        "signing": {
            "identity_regexp": f"^https://github.com/{REPO}/.github/workflows/.+",
            "oidc_issuer": "https://token.actions.githubusercontent.com",
        },
        "plugins": plugins,
    }
    OUT.write_text(json.dumps(document, indent=2) + "\n")
    print(f"wrote {OUT.relative_to(ROOT)} with {len(plugins)} plugin(s)")


if __name__ == "__main__":
    main()
