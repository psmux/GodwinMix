//! `SKILL.md` in the Agent Skills format, and the check the harness runs.
//!
//! The rules: YAML frontmatter between two `---` lines carrying `name` and
//! `description`; the description is at most 1,024 characters and is loaded
//! into every agent context always, so it pays rent; the body is loaded only
//! when the skill is used and must stay under 5,000 tokens.
//!
//! The frontmatter is read with a small hand written parser rather than a YAML
//! crate. Two scalar keys do not justify a dependency, and this way the SDK
//! stays on serde, serde_json and toml.

use serde::Serialize;

/// The description limit from the Agent Skills format.
pub const MAX_DESCRIPTION_CHARS: usize = 1024;
/// The body budget. Tokens are estimated, so the check is a guide rail, not a
/// tokeniser.
pub const MAX_BODY_TOKENS: usize = 5000;

/// A parsed SKILL.md.
#[derive(Debug, Clone, Serialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    /// Any other frontmatter key, kept so a future field is not lost.
    pub extra: Vec<(String, String)>,
    pub body: String,
}

impl Skill {
    /// Roughly how many tokens the body costs. Four characters per token is the
    /// ratio the budget in 03 section 4 assumes.
    pub fn body_tokens(&self) -> usize {
        self.body.chars().count().div_ceil(4)
    }
}

/// One thing wrong with a SKILL.md.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub message: String,
}

fn problem(message: impl Into<String>) -> Problem {
    Problem {
        message: message.into(),
    }
}

/// Split the frontmatter from the body. `None` if there is no frontmatter.
pub fn parse(text: &str) -> Option<Skill> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    let mut name = String::new();
    let mut description = String::new();
    let mut extra: Vec<(String, String)> = Vec::new();
    let mut closed = false;
    let mut consumed = 1;
    for line in lines.by_ref() {
        consumed += 1;
        if line.trim() == "---" {
            closed = true;
            break;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_string();
        let value = unquote(value.trim());
        match key.as_str() {
            "name" => name = value,
            "description" => description = value,
            _ => extra.push((key, value)),
        }
    }
    if !closed {
        return None;
    }
    let body: String = text
        .lines()
        .skip(consumed)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    Some(Skill {
        name,
        description,
        extra,
        body,
    })
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

/// Every problem with a SKILL.md, in reading order.
pub fn validate(text: &str) -> Vec<Problem> {
    let Some(skill) = parse(text) else {
        return vec![problem(
            "SKILL.md needs YAML frontmatter: a line '---', then name and description, \
             then a closing '---'.",
        )];
    };
    let mut out = Vec::new();
    if skill.name.trim().is_empty() {
        out.push(problem("the frontmatter has no 'name'."));
    } else if !super::manifest::is_slug(&skill.name) {
        out.push(problem(format!(
            "name '{}' is not a slug. Use lower case letters, digits and hyphens.",
            skill.name
        )));
    }
    if skill.description.trim().is_empty() {
        out.push(problem(
            "the frontmatter has no 'description'. It is the only part loaded into every \
             context, so it must say what the skill does and when to use it.",
        ));
    } else {
        let chars = skill.description.chars().count();
        if chars > MAX_DESCRIPTION_CHARS {
            out.push(problem(format!(
                "the description is {chars} characters; the limit is {MAX_DESCRIPTION_CHARS}."
            )));
        }
        let lower = skill.description.to_lowercase();
        if !lower.contains("use when")
            && !lower.contains("use it when")
            && !lower.contains("use this when")
        {
            out.push(problem(
                "the description does not say when to use the skill. Add a sentence starting \
                 'Use when ...'; that is what decides whether a model loads it.",
            ));
        }
    }
    if skill.body.trim().is_empty() {
        out.push(problem("the body is empty."));
    } else {
        let tokens = skill.body_tokens();
        if tokens > MAX_BODY_TOKENS {
            out.push(problem(format!(
                "the body is about {tokens} tokens; the budget is {MAX_BODY_TOKENS}. \
                 Move the detail into a reference page and link to it."
            )));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLOCK: &str = "---\n\
name: clock-source\n\
description: Adds a clock source to GodwinMix that draws the current time on a solid background. Use when an operator asks for a clock, a countdown, or a test picture with the time on it.\n\
---\n\
\n\
# Clock source\n\
\n\
Add it with source.add.\n";

    #[test]
    fn the_worked_example_parses_and_passes() {
        let skill = parse(CLOCK).unwrap();
        assert_eq!(skill.name, "clock-source");
        assert!(skill.body.starts_with("# Clock source"));
        assert!(validate(CLOCK).is_empty(), "{:#?}", validate(CLOCK));
    }

    #[test]
    fn no_frontmatter_is_one_clear_problem() {
        let got = validate("# Clock\n\nno frontmatter here\n");
        assert_eq!(got.len(), 1);
        assert!(got[0].message.contains("frontmatter"));
    }

    #[test]
    fn unclosed_frontmatter_is_caught() {
        let got = validate("---\nname: x\ndescription: Use when testing.\n\n# body\n");
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn a_long_description_is_caught_with_the_number() {
        let text = format!(
            "---\nname: x\ndescription: Use when {}\n---\n\nbody\n",
            "a".repeat(MAX_DESCRIPTION_CHARS)
        );
        let got = validate(&text);
        assert!(got.iter().any(|p| p.message.contains("the limit is")), "{got:#?}");
    }

    #[test]
    fn a_description_that_never_says_when_is_caught() {
        let text = "---\nname: x\ndescription: Does a thing.\n---\n\nbody\n";
        let got = validate(text);
        assert!(got.iter().any(|p| p.message.contains("when to use")), "{got:#?}");
    }

    #[test]
    fn a_body_over_budget_is_caught() {
        let text = format!(
            "---\nname: x\ndescription: Use when testing.\n---\n\n{}\n",
            "word ".repeat(6000)
        );
        let got = validate(&text);
        assert!(got.iter().any(|p| p.message.contains("budget")), "{got:#?}");
    }

    #[test]
    fn quotes_around_values_come_off() {
        let text = "---\nname: \"my-skill\"\ndescription: 'Use when quoted.'\n---\n\nbody\n";
        let skill = parse(text).unwrap();
        assert_eq!(skill.name, "my-skill");
        assert_eq!(skill.description, "Use when quoted.");
    }

    #[test]
    fn extra_keys_are_kept() {
        let text = "---\nname: x\ndescription: Use when testing.\nlicense: MIT\n---\n\nbody\n";
        let skill = parse(text).unwrap();
        assert_eq!(skill.extra, vec![("license".to_string(), "MIT".to_string())]);
    }

    #[test]
    fn a_name_that_is_not_a_slug_is_caught() {
        let text = "---\nname: My Skill\ndescription: Use when testing.\n---\n\nbody\n";
        let got = validate(text);
        assert!(got.iter().any(|p| p.message.contains("slug")), "{got:#?}");
    }
}
