//! `oci:ghcr.io/x/gmx-ai-vision:1.0`: documented, and refused.
//!
//! 06 section 2 lists OCI as the form for plugins with heavy dependencies, and
//! says they run in a container on a node. Running one means a container
//! runtime under the core's supervision, a network for the media transport
//! that is not a pipe, a lifecycle that survives a restart of the daemon, and
//! a whole second set of teardown bugs. That is 04's work, not this one's, and
//! a half implementation that starts containers nobody reaps is worse than
//! nothing.
//!
//! So the form parses, `gmx plugin add` recognises it, and the refusal says
//! what will happen and what to do today. The refusal changes depending on
//! whether a runtime is installed, because "install podman" is useless advice
//! to somebody who already has it.

use super::Fetched;
use anyhow::Result;

/// Runtimes worth naming, in the order they are looked for.
const RUNTIMES: &[&str] = &["podman", "docker", "nerdctl"];

pub fn refuse(reference: &str) -> Result<Fetched> {
    let runtime = RUNTIMES.iter().find(|r| super::have(r));
    let image = reference.split('/').next_back().unwrap_or(reference);
    match runtime {
        Some(found) => anyhow::bail!(
            "`oci:{reference}` is not installable by this core yet, and {found} is installed \
             here so it will be the runtime when it is. A container plugin is placed on a \
             node rather than run beside the core, which is 04's work.\n\
             Today: pull it yourself and run it as a node, or ask the author whether \
             {image} also publishes a release asset, which installs with \
             `gmx plugin add <owner>/<repo>`."
        ),
        None => anyhow::bail!(
            "`oci:{reference}` needs a container runtime and there is none on this machine \
             (looked for {}). Container plugins are not installable by this core yet in any \
             case: they are placed on a node, which is 04's work.\n\
             Today: ask the author whether {image} also publishes a release asset, which \
             installs with `gmx plugin add <owner>/<repo>` and needs no runtime at all.",
            RUNTIMES.join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_refusal_names_the_next_step_either_way() {
        let err = refuse("ghcr.io/x/gmx-ai-vision:1.0").expect_err("oci is refused");
        let text = format!("{err}");
        assert!(text.contains("gmx plugin add <owner>/<repo>"), "{text}");
        assert!(text.contains("gmx-ai-vision"), "{text}");
    }
}
