//! The glue the two export macros share.
//!
//! A component is a set of exported functions with no state of its own, so the
//! plugin's state lives in a `RefCell` beside them. That is safe here in a way
//! it would not be in the core: a component instance is single threaded by
//! construction, one store, one worker, one call at a time.
//!
//! Nothing in this file is written by hand twice. `export_service!` drives the
//! `service` interface and stubs `transition`; `export_transition!` does the
//! reverse. The stub answers -32601 with the name of the interface the plugin
//! did implement, which is the error an author gets when a manifest says
//! `kind = "transition"` over a plugin that is a service.

/// The state holder, the glue type, and the three accessors around them.
///
/// Emitted into the plugin's own crate, because a `Guest` impl on a type this
/// crate owned would be an orphan: the trait is here, so the type has to be
/// there.
#[doc(hidden)]
#[macro_export]
macro_rules! __state {
    ($t:ty) => {
        /// The type the generated export macro is given.
        pub struct GmxGlue;

        thread_local! {
            static GMX_STATE: core::cell::RefCell<Option<$t>> =
                const { core::cell::RefCell::new(None) };
        }
    };
}

/// Put the plugin's state away after a handshake.
#[doc(hidden)]
#[macro_export]
macro_rules! __put {
    ($state:expr) => {
        GMX_STATE.with(|cell| *cell.borrow_mut() = Some($state))
    };
}

/// Run a closure over the state, or answer -32001 when there is none.
///
/// There is none only before `initialize` has answered, which the host never
/// arranges; saying so is better than unwrapping.
#[doc(hidden)]
#[macro_export]
macro_rules! __with {
    (|$name:ident : &mut $t:ty| $body:expr) => {
        GMX_STATE.with(|cell| match cell.borrow_mut().as_mut() {
            Some($name) => {
                let $name: &mut $t = $name;
                $body
            }
            None => Err($crate::wire::error(
                -32001,
                "the plugin has not finished its handshake, so it cannot answer yet".to_string(),
            )),
        })
    };
}

/// The same, with a plain value rather than an error when there is no state.
#[doc(hidden)]
#[macro_export]
macro_rules! __with_or {
    (|$name:ident : &mut $t:ty| $body:expr, $fallback:expr) => {
        GMX_STATE.with(|cell| match cell.borrow_mut().as_mut() {
            Some($name) => {
                let $name: &mut $t = $name;
                $body
            }
            None => $fallback,
        })
    };
}

/// The half a service plugin does not implement.
#[doc(hidden)]
#[macro_export]
macro_rules! __stub_transition {
    () => {
        impl $crate::bindings::exports::godwinmix::plugin::transition::Guest for GmxGlue {
            fn initialize(
                _hello: $crate::bindings::godwinmix::plugin::types::Hello,
            ) -> Result<
                $crate::bindings::godwinmix::plugin::types::Ready,
                $crate::bindings::godwinmix::plugin::types::Error,
            > {
                Err($crate::wire::error(
                    -32601,
                    "this component is a service, not a transition. Its manifest provide \
                     should say kind = \"service\"."
                        .to_string(),
                ))
            }

            fn render(
                _request_json: String,
            ) -> Result<
                $crate::bindings::exports::godwinmix::plugin::transition::Answer,
                $crate::bindings::godwinmix::plugin::types::Error,
            > {
                Err($crate::wire::error(
                    -32601,
                    "this component is a service and answers no `render`".to_string(),
                ))
            }
        }
    };
}

/// The half a transition plugin does not implement.
#[doc(hidden)]
#[macro_export]
macro_rules! __stub_service {
    () => {
        impl $crate::bindings::exports::godwinmix::plugin::service::Guest for GmxGlue {
            fn initialize(
                _hello: $crate::bindings::godwinmix::plugin::types::Hello,
            ) -> Result<
                $crate::bindings::godwinmix::plugin::types::Ready,
                $crate::bindings::godwinmix::plugin::types::Error,
            > {
                Err($crate::wire::error(
                    -32601,
                    "this component is a transition, not a service. Its manifest provide \
                     should say kind = \"transition\"."
                        .to_string(),
                ))
            }

            fn configure(
                _params_json: String,
            ) -> Result<
                $crate::bindings::godwinmix::plugin::types::Configured,
                $crate::bindings::godwinmix::plugin::types::Error,
            > {
                Ok($crate::wire::configured(&$crate::Configured::applied()))
            }

            fn health() -> $crate::bindings::godwinmix::plugin::types::HealthReport {
                $crate::wire::health(&$crate::Health::default())
            }

            fn tool_call(
                name: String,
                _arguments_json: String,
            ) -> Result<String, $crate::bindings::godwinmix::plugin::types::Error> {
                Err($crate::wire::error(
                    -32601,
                    format!("this component is a transition and has no tool called `{name}`"),
                ))
            }

            fn hook(
                name: String,
                _payload_json: String,
            ) -> Result<String, $crate::bindings::godwinmix::plugin::types::Error> {
                Err($crate::wire::error(
                    -32601,
                    format!("this component is a transition and answers no `{name}` hook"),
                ))
            }

            fn shutdown(_reason: String) {}
        }
    };
}

/// The body of both export macros.
#[doc(hidden)]
#[macro_export]
macro_rules! __glue {
    ($t:ty, service) => {
        $crate::__state!($t);

        impl $crate::bindings::exports::godwinmix::plugin::service::Guest for GmxGlue {
            fn initialize(
                hello: $crate::bindings::godwinmix::plugin::types::Hello,
            ) -> Result<
                $crate::bindings::godwinmix::plugin::types::Ready,
                $crate::bindings::godwinmix::plugin::types::Error,
            > {
                let hello = $crate::wire::hello(hello);
                match <$t as $crate::Service>::initialize(&hello) {
                    Ok((state, ready)) => {
                        $crate::__put!(state);
                        Ok($crate::wire::ready(&ready))
                    }
                    Err(why) => Err($crate::wire::error(-32001, why)),
                }
            }

            fn configure(
                params_json: String,
            ) -> Result<
                $crate::bindings::godwinmix::plugin::types::Configured,
                $crate::bindings::godwinmix::plugin::types::Error,
            > {
                let params = $crate::wire::json(&params_json);
                $crate::__with!(|state: &mut $t| {
                    match $crate::Service::configure(state, &params) {
                        Ok(c) => Ok($crate::wire::configured(&c)),
                        Err(why) => Err($crate::wire::error(-32602, why)),
                    }
                })
            }

            fn health() -> $crate::bindings::godwinmix::plugin::types::HealthReport {
                $crate::__with_or!(
                    |state: &mut $t| $crate::wire::health(&$crate::Service::health(state)),
                    $crate::wire::health(&$crate::Health::degraded(
                        "the plugin has not finished its handshake"
                    ))
                )
            }

            fn tool_call(
                name: String,
                arguments_json: String,
            ) -> Result<String, $crate::bindings::godwinmix::plugin::types::Error> {
                let arguments = $crate::wire::json(&arguments_json);
                $crate::__with!(|state: &mut $t| {
                    match $crate::Service::tool_call(state, &name, &arguments) {
                        Ok(v) => Ok(v.to_string()),
                        Err(why) => Err($crate::wire::error(-32602, why)),
                    }
                })
            }

            fn hook(
                name: String,
                payload_json: String,
            ) -> Result<String, $crate::bindings::godwinmix::plugin::types::Error> {
                let payload = $crate::wire::json(&payload_json);
                $crate::__with!(|state: &mut $t| {
                    match $crate::Service::hook(state, &name, &payload) {
                        Ok(v) => Ok(v.to_string()),
                        Err(why) => Err($crate::wire::error(-32001, why)),
                    }
                })
            }

            fn shutdown(reason: String) {
                let _ = $crate::__with!(|state: &mut $t| {
                    $crate::Service::shutdown(state, &reason);
                    Ok::<(), $crate::bindings::godwinmix::plugin::types::Error>(())
                });
            }
        }

        $crate::__stub_transition!();
        $crate::bindings::export_plugin!(GmxGlue with_types_in $crate::bindings);
    };

    ($t:ty, transition) => {
        $crate::__state!($t);

        impl $crate::bindings::exports::godwinmix::plugin::transition::Guest for GmxGlue {
            fn initialize(
                hello: $crate::bindings::godwinmix::plugin::types::Hello,
            ) -> Result<
                $crate::bindings::godwinmix::plugin::types::Ready,
                $crate::bindings::godwinmix::plugin::types::Error,
            > {
                let hello = $crate::wire::hello(hello);
                match <$t as $crate::Transition>::initialize(&hello) {
                    Ok((state, ready)) => {
                        $crate::__put!(state);
                        Ok($crate::wire::ready(&ready))
                    }
                    Err(why) => Err($crate::wire::error(-32001, why)),
                }
            }

            fn render(
                request_json: String,
            ) -> Result<
                $crate::bindings::exports::godwinmix::plugin::transition::Answer,
                $crate::bindings::godwinmix::plugin::types::Error,
            > {
                let request = $crate::wire::json(&request_json);
                $crate::__with!(|state: &mut $t| {
                    match $crate::Transition::render(state, &request) {
                        Ok(answer) => Ok($crate::wire::answer(answer)),
                        Err(why) => Err($crate::wire::error(-32001, why)),
                    }
                })
            }
        }

        $crate::__stub_service!();
        $crate::bindings::export_plugin!(GmxGlue with_types_in $crate::bindings);
    };
}
