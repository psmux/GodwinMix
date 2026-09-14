# Skills

Two Agent Skills that ship with the CLI. `gmx skill install` drops them into
Claude Code, Codex or Gemini CLI, so an agent asked to run a show or write a
plugin already knows the conventions instead of guessing them.

| Skill | For |
|---|---|
| [godwinmix-operate](godwinmix-operate/SKILL.md) | running a live mixer: connecting, the state document, taking a source, when to look at a picture rather than numbers, safety and rehearsal |
| [godwinmix-develop](godwinmix-develop/SKILL.md) | writing a plugin: the manifest, the templates, the protocol from a standard library, the media contract, the conformance harness, the listing path |

`gmx skill install` does not exist yet. Until it does, copy the directory into
your agent's skills folder.

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
its `gmx-plugin.toml`. Those are about that plugin; these two are about the
mixer. The templates under `templates/` each have one to start from.
