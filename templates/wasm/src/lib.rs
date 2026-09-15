//! {{description}}
//!
//! A GodwinMix plugin as a WebAssembly component. It runs inside the core,
//! sandboxed, with a fuel allowance and a deadline per call. It touches no
//! media and cannot: tier W is for logic.
//!
//! Build it with `./check`, or by hand:
//!
//! ```text
//! cargo build --release --target wasm32-wasip2
//! cp target/wasm32-wasip2/release/{{name_snake}}.wasm plugin.wasm
//! gmx plugin add .
//! ```

use godwinmix_sdk_wasm::{allow, export_service, log, refuse, Hello, Level, Ready, Service};
use serde_json::{json, Value};

// The type is named after the plugin, so it reads like the plugin in a stack
// trace. Rust would rather it were CamelCase; the plugin's own name matters
// more here than the lint does.
#[allow(non_camel_case_types)]
pub struct {{name_snake}} {
    /// Read off `params` at the handshake. Write your own settings into
    /// schemas/settings.json and read them here.
    example_ms: u64,
}

impl Service for {{name_snake}} {
    fn initialize(hello: &Hello) -> Result<(Self, Ready), String> {
        let example_ms = hello.param_u64("example_ms", 1_000);
        log(Level::Info, format!("starting on {}", hello.instance));
        Ok((
            {{name_snake}} { example_ms },
            // Name every hook and tool here. The core only calls what this
            // list names, so a hook the manifest asks for and this forgets is
            // never fired.
            Ready::new("{{name}}", "0.1.0").hook("take.before"),
        ))
    }

    fn hook(&mut self, name: &str, payload: &Value) -> Result<Value, String> {
        if name != "take.before" {
            return Ok(json!({}));
        }
        // Answer `refuse(..)` to stop the take. The reason is the whole of
        // what an operator or an agent reads, so name what is wrong and what
        // to do about it, in that order.
        let _ = (payload, self.example_ms);
        if false {
            return Ok(refuse("say what is wrong and what to do about it"));
        }
        Ok(allow())
    }
}

export_service!({{name_snake}});
