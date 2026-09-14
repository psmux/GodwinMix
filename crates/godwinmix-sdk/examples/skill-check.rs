//! Validate one or more `SKILL.md` files, the way harness check 7 does.
//!
//!     cargo run -p godwinmix-sdk --example skill-check -- skills/*/SKILL.md
//!
//! It prints the name, the description length against the 1,024 character limit
//! and the body's rough token cost against the 5,000 token budget, then every
//! problem. Exit 1 if anything is wrong, so it drops into a check script.

use godwinmix_sdk::skill::{self, MAX_BODY_TOKENS, MAX_DESCRIPTION_CHARS};

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!("usage: skill-check <SKILL.md> [more...]");
        std::process::exit(2);
    }
    let mut bad = false;
    for path in paths {
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) => {
                println!("{path}: could not read it: {e}");
                bad = true;
                continue;
            }
        };
        match skill::parse(&text) {
            Some(parsed) => println!(
                "{path}: {}, description {}/{} characters, body about {}/{} tokens",
                parsed.name,
                parsed.description.chars().count(),
                MAX_DESCRIPTION_CHARS,
                parsed.body_tokens(),
                MAX_BODY_TOKENS
            ),
            None => println!("{path}: no frontmatter"),
        }
        for problem in skill::validate(&text) {
            println!("  {}", problem.message);
            bad = true;
        }
    }
    if bad {
        std::process::exit(1);
    }
}
