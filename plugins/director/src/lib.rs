//! `gmx-director`: an automatic director for GodwinMix, as a `service` plugin.
//!
//! It reads `agent.state` every couple of seconds, decides which source should
//! be on air, and calls `program.take`. Out of the box it decides on rules and
//! needs no model, no API key and no network beyond the mixer. Point the `llm`
//! setting at a command and it asks that command for an opinion each cycle,
//! and then puts the answer through exactly the same rules before anything
//! reaches the mixer.
//!
//! `examples/ai-director.py` in this repository is the teaching version of the
//! same loop: 290 lines of Python, one file, with the Anthropic SDK in it and
//! nothing hidden. It stays. Read it to understand the loop; run this one on a
//! show, because it is a process the core supervises, it restarts when it
//! falls over, its settings are a schema every surface renders, and it holds
//! the programme on rules when the model is slow or wrong.
//!
//! | Module | What it is |
//! |---|---|
//! | [`rules`] | the decision, with no clock, socket or model in it |
//! | [`settings`] | `schemas/director.json`, read into a struct |
//! | [`llm`] | the prompt, the command, and reading its answer |
//! | [`service`] | `agent.state` in, `program.take` out |

pub mod llm;
pub mod rules;
pub mod service;
pub mod settings;

pub use rules::{Decision, Shot, View};
pub use settings::Settings;
