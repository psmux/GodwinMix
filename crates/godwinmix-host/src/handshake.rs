//! The `initialize` exchange: the api range check and the transport choice.
//!
//! The plugin sends first. The core answers with the canvas, the transport it
//! picked, the media address for that transport, and the params it validated.
//! A plugin that has not sent `initialize` within five seconds is killed, and
//! one naming an `api` outside the supported range is refused, both with the
//! reason in `event/plugin.state`.

use godwinmix_protocol::plugin::manifest::Manifest;
use godwinmix_protocol::plugin::wire::{Canvas, Initialize, Ready, Transport};
use std::time::Duration;

/// How long a plugin has to send `initialize`.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);

/// What the handshake settled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Negotiated {
    pub transport: Transport,
    /// The unixfd socket path or shm name. Empty for a container on a pipe.
    pub media: String,
    pub api: u32,
    /// What the plugin said its own delay is, if it said anything.
    pub latency_ms: Option<u32>,
}

/// Is this plugin's `api` one this core can host?
///
/// Neovim's rule: a plugin at level N loads on any core whose
/// `api_compatible <= N <= api_level`.
pub fn check_api(api: u32, level: u32, compatible: u32) -> anyhow::Result<()> {
    anyhow::ensure!(
        api >= compatible,
        "the plugin was written against api {api} and this core answers {compatible} and \
         above. Update the plugin, or run a core from the release table that still speaks {api}."
    );
    anyhow::ensure!(
        api <= level,
        "the plugin needs api {api} and this core speaks up to {level}. Update the core."
    );
    Ok(())
}

/// Pick the transport, given what the plugin declared and what this machine
/// can carry.
///
/// Preference is `unixfd`, then `shm`, then `container`, and a transport this
/// build cannot open is skipped rather than chosen and then failed. Windows
/// gets the container and, when a plugin declared nothing else, an error that
/// names it.
pub fn negotiate_transport(declared: &[Transport]) -> anyhow::Result<Transport> {
    negotiate_transport_for(declared, true)
}

/// The same, for a provide that may carry no media at all.
///
/// A `service`, a `device` and a `transition` have no media contract: they are
/// control plane only, and the manifest validator does not ask them for a
/// transport. So one of those is recorded as `container`, which is the
/// transport whose media address is empty, and no socket is ever made.
/// Anything that does carry media must still declare one, and the error says
/// which to pick.
///
/// What it declared is not consulted for those, because `initialize` is sent
/// once per process and names what the process can do, not what this provide
/// needs. One binary serving both `camera/source` and `camera/devices` says
/// `unixfd, container` either way, and a device instance given the socket
/// asks for a media address nobody can give it: the handshake then fails and
/// the mixer has no camera to discover. That is what it did.
pub fn negotiate_transport_for(
    declared: &[Transport],
    carries_media: bool,
) -> anyhow::Result<Transport> {
    if !carries_media {
        return Ok(Transport::Container);
    }
    anyhow::ensure!(
        !declared.is_empty(),
        "the plugin declared no transports. Every source declares at least one; 'container' \
         works everywhere and is the safe first choice."
    );
    if let Some(t) = Transport::ORDER
        .iter()
        .copied()
        .find(|t| declared.contains(t) && t.available_here())
    {
        return Ok(t);
    }
    let names: Vec<&str> = declared.iter().map(|t| t.as_str()).collect();
    anyhow::bail!(
        "the plugin offers {} and this build can carry none of them on {}. `unixfd` and `shm` \
         need Unix sockets; add 'container' to the provide's transports and the plugin runs \
         everywhere.",
        names.join(", "),
        std::env::consts::OS
    )
}

/// Everything the core decides when a plugin says hello.
///
/// `media_for` is asked for an address only once a socket transport has been
/// chosen, so nothing creates a socket for a plugin that turns out to want a
/// pipe.
pub fn negotiate(
    hello: &Initialize,
    manifest: Option<&Manifest>,
    level: u32,
    compatible: u32,
    media_for: impl FnOnce(Transport) -> anyhow::Result<String>,
) -> anyhow::Result<Negotiated> {
    negotiate_for(hello, manifest, level, compatible, true, media_for)
}

/// The same, told whether this provide carries media at all.
pub fn negotiate_for(
    hello: &Initialize,
    manifest: Option<&Manifest>,
    level: u32,
    compatible: u32,
    carries_media: bool,
    media_for: impl FnOnce(Transport) -> anyhow::Result<String>,
) -> anyhow::Result<Negotiated> {
    check_api(hello.api, level, compatible)?;
    if let Some(manifest) = manifest {
        anyhow::ensure!(
            manifest.plugin.name == hello.plugin,
            "the process said it is '{}' and the manifest at its root says '{}'. One of the \
             two is the wrong directory.",
            hello.plugin,
            manifest.plugin.name
        );
    }
    let transport = negotiate_transport_for(&hello.transports, carries_media)?;
    let media = match transport {
        Transport::Container => String::new(),
        other => media_for(other)?,
    };
    Ok(Negotiated { transport, media, api: hello.api, latency_ms: None })
}

/// The answer the core writes back, in the shape 03 section 6 documents.
///
/// Eight arguments, because the handshake answer has eight fields and every
/// one is decided somewhere different: the version by the build, the levels by
/// the protocol crate, the canvas by the config, the transport by what was
/// negotiated, and the last three by the instance being started. A struct to
/// carry them would be the same eight names written twice.
#[allow(clippy::too_many_arguments)]
pub fn ready(
    version: &str,
    level: u32,
    compatible: u32,
    canvas: Canvas,
    negotiated: &Negotiated,
    instance: &str,
    provide: &str,
    params: serde_json::Value,
) -> Ready {
    Ready {
        core: "godwinmix".into(),
        version: version.to_string(),
        api_level: level,
        api_compatible: compatible,
        canvas,
        transport: negotiated.transport,
        media: negotiated.media.clone(),
        instance: instance.to_string(),
        provide: provide.to_string(),
        params,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello(transports: Vec<Transport>, api: u32) -> Initialize {
        Initialize {
            plugin: "clock".into(),
            version: "0.1.0".into(),
            api,
            transports,
            provides: Vec::new(),
        }
    }

    #[test]
    fn a_plugin_from_the_future_is_refused_by_number() {
        let err = check_api(3, 1, 1).expect_err("api 3 on a level 1 core");
        assert!(format!("{err}").contains("Update the core"), "{err}");
        let err = check_api(1, 3, 2).expect_err("api 1 on a core that dropped it");
        assert!(format!("{err}").contains("Update the plugin"), "{err}");
        check_api(2, 3, 1).expect("a level in the middle of the range loads");
    }

    #[test]
    fn a_container_is_chosen_when_it_is_all_that_is_offered() {
        let t = negotiate_transport(&[Transport::Container]).expect("container works everywhere");
        assert_eq!(t, Transport::Container);
    }

    #[test]
    fn the_fastest_transport_this_machine_can_carry_wins() {
        let t = negotiate_transport(&[Transport::Container, Transport::Shm, Transport::Unixfd])
            .expect("something is available");
        if cfg!(unix) {
            assert_eq!(t, Transport::Unixfd, "a unix box takes the zero copy path");
        } else {
            assert_eq!(t, Transport::Container, "windows has the pipe and says so");
        }
    }

    #[test]
    fn a_plugin_with_no_transports_is_told_which_one_to_add() {
        let err = negotiate_transport(&[]).expect_err("nothing declared");
        assert!(format!("{err}").contains("container"), "{err}");
    }

    #[cfg(not(unix))]
    #[test]
    fn windows_refuses_a_socket_only_plugin_and_names_the_way_forward() {
        let err = negotiate_transport(&[Transport::Unixfd, Transport::Shm])
            .expect_err("neither is available here");
        assert!(format!("{err}").contains("container"), "{err}");
    }

    #[test]
    fn a_device_provide_gets_the_container_whatever_the_process_declared() {
        // The camera plugin: one binary, a source provide that wants the
        // socket and a device provide that discovers cameras. `initialize`
        // names the process's transports, so the device instance arrives here
        // carrying `unixfd` and must still be given the container.
        let t = negotiate_transport_for(&[Transport::Unixfd, Transport::Container], false)
            .expect("a device provide needs no media address");
        assert_eq!(t, Transport::Container);
        assert_eq!(negotiate_transport_for(&[], false).unwrap(), Transport::Container);
    }

    #[test]
    fn a_container_plugin_is_given_no_media_address() {
        let n = negotiate(&hello(vec![Transport::Container], 1), None, 1, 1, |_| {
            panic!("a container plugin must not make a socket")
        })
        .expect("the handshake settles");
        assert_eq!(n.transport, Transport::Container);
        assert!(n.media.is_empty());
    }

    #[test]
    fn the_answer_carries_what_the_documented_handshake_carries() {
        let n = Negotiated {
            transport: Transport::Container,
            media: String::new(),
            api: 1,
            latency_ms: None,
        };
        let answer = ready(
            "0.2.0",
            1,
            1,
            Canvas::new(1280, 720, 30),
            &n,
            "cam1",
            "source",
            serde_json::json!({"timezone": "UTC"}),
        );
        let text = serde_json::to_value(&answer).expect("it serialises");
        assert_eq!(text["core"], "godwinmix");
        assert_eq!(text["api_level"], 1);
        assert_eq!(text["canvas"]["width"], 1280);
        assert_eq!(text["instance"], "cam1");
        assert_eq!(text["transport"], "container");
    }
}
