//! The `Service` trait and the macro that exports it.
//!
//! The trait mirrors `godwinmix_sdk::Plugin` for a service: the same methods,
//! the same JSON, the same defaults. Only `initialize` has to be written; a
//! plugin that answers no tools and no hooks can leave the rest alone.

use crate::{Configured, Health, Hello};
use serde_json::Value;

/// A control plane plugin, as a component.
pub trait Service: Sized {
    /// The handshake. Build the plugin's state and say what it answers to.
    ///
    /// An `Err` fails the instance and the message reaches
    /// `event/plugin.state`, so it should say what was wrong with the params
    /// or the grant rather than "failed".
    fn initialize(hello: &Hello) -> Result<(Self, crate::Ready), String>;

    /// New params, the whole validated object.
    ///
    /// The default takes them and does nothing, which is right for a plugin
    /// whose behaviour is read off `params` at the moment it is used. A plugin
    /// that caches something derived from them overrides this, and one that
    /// cannot change without a restart answers [`Configured::restart`].
    fn configure(&mut self, _params: &Value) -> Result<Configured, String> {
        Ok(Configured::applied())
    }

    /// How it is. The default is well.
    fn health(&mut self) -> Health {
        Health::default()
    }

    /// One of the plugin's `[[tools]]`, in MCP's shape.
    fn tool_call(&mut self, name: &str, _arguments: &Value) -> Result<Value, String> {
        Err(format!("this plugin has no tool called `{name}`"))
    }

    /// One of 03 section 8's hooks.
    ///
    /// `payload` is the hook envelope, `{hook, ts, payload}`. A `take.before`
    /// answers [`crate::allow`] or [`crate::refuse`]; every other hook answers
    /// `{}` because it cannot change the decision.
    fn hook(&mut self, name: &str, _payload: &Value) -> Result<Value, String> {
        let _ = name;
        Ok(serde_json::json!({}))
    }

    /// The core is going away.
    fn shutdown(&mut self, _reason: &str) {}
}

/// Export a [`Service`] as a component.
///
/// Writes the glue, holds the plugin's state, and hands the type to the
/// generated export macro. The component it produces also exports the
/// `transition` interface, answering -32601 there, so the same binary
/// satisfies either of the host's two worlds.
#[macro_export]
macro_rules! export_service {
    ($t:ty) => {
        const _: () = {
            $crate::__glue!($t, service);
        };
    };
}
