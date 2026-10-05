//! What is wrong with a template, said so that the next attempt is right.

pub use godwinmix_protocol::graphics::TemplateProblem;

pub type Problems = Vec<TemplateProblem>;

/// Something that stops the template being saved or drawn.
pub fn error(problem: &str, fix: &str) -> TemplateProblem {
    TemplateProblem { level: "error".into(), problem: problem.into(), fix: fix.into() }
}

/// Something drawn as it is, that is probably not what was meant.
pub fn warning(problem: &str, fix: &str) -> TemplateProblem {
    TemplateProblem { level: "warning".into(), problem: problem.into(), fix: fix.into() }
}

/// True when none of `problems` is an error.
pub fn ok(problems: &[TemplateProblem]) -> bool {
    !problems.iter().any(|p| p.level == "error")
}

/// The errors in one message, each with its fix, for a refusal.
pub fn message(name: &str, problems: &[TemplateProblem]) -> String {
    let errors: Vec<String> =
        problems.iter().filter(|p| p.level == "error").map(|p| format!("{}. Fix: {}", p.problem, p.fix)).collect();
    format!("the HTML template {name} cannot be drawn: {}", errors.join("; "))
}
