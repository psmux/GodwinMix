# Skills

Three Agent Skills that ship with the CLI. `gmx skill install` drops them into
Claude Code, opencode, pi, Codex or Gemini CLI, so an agent asked to run a show, write a
plugin or design a graphic already knows the conventions instead of guessing
them.

| Skill | For |
|---|---|
| [godwinmix-operate](godwinmix-operate/SKILL.md) | running a live mixer: connecting, the state document, taking a source, when to look at a picture rather than numbers, safety and rehearsal |
| [godwinmix-develop](godwinmix-develop/SKILL.md) | writing a plugin: the manifest, the templates, the protocol from a standard library, the media contract, the conformance harness, the listing path |
| [godwinmix-design](godwinmix-design/SKILL.md) | designing a graphic: the template pack, an SVG template with fields and shrink to fit, safe areas, putting it on a scene with an enter and an exit, looking at it, changing its words on air |

```sh
gmx skill install --for claude          # or opencode, pi, codex, gemini
gmx skill install --for claude --print  # what it would write, writing nothing
```

## The format

Agent Skills: YAML frontmatter with `name` and `description`, then a Markdown
body. The description is loaded into every context always, so it has to say what
the skill does **and when to use it** inside 1,024 characters. The body loads
only when the skill is used and stays under 5,000 tokens.

Check one before committing it:

```sh
cargo run -p godwinmix-sdk --example skill-check -- skills/*/SKILL.md
```

That is the same validation the conformance harness runs over a plugin's own
`SKILL.md` files, so a skill that passes here passes there.

## Plugins ship their own

Every plugin carries a `SKILL.md` per provided kind, named by `skill = ...` in
its `gmx-plugin.toml`. Those are about that plugin; these three are about the
mixer. The templates under `templates/` each have one to start from.
