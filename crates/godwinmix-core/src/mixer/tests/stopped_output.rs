use super::*;

/// What `output.stop` and `output.start` promise: stopping detaches the
/// destination and keeps its address, the status says `stopped`, the runtime
/// store keeps it with `enabled = false`, and starting attaches it again
/// under the same id. Removing a stopped one forgets it.
#[tokio::test(flavor = "multi_thread")]
async fn a_stopped_output_keeps_its_address_and_starts_again_under_its_id() {
    let _ = gst::init();
    let (mut mix, _handle, _rx, _bus) =
        Mixer::build(programme_config(crate::config::Accel::Software)).expect("mixer builds");
    mix.start().expect("the programme starts");
    let uri = format!("rtmp://127.0.0.1:{}/live/the-key", super::slow_output::silent_server());
    mix.add_output(&OutputConfig::bare("youtube", &uri)).expect("the output attaches");

    mix.handle(Command::StopOutput("youtube".into(), None)).expect("it stops");
    let stopped = mix.status();
    let built_while_stopped = mix.outputs.len();
    let kept = mix.runtime_configs();
    mix.handle(Command::StopOutput("youtube".into(), None)).expect("a second stop is not an error");
    mix.handle(Command::StartOutput("youtube".into(), None)).expect("it starts");
    let started = mix.status();
    mix.handle(Command::StopOutput("youtube".into(), None)).expect("it stops again");
    mix.remove_output(&"youtube".to_string()).expect("a stopped output can be removed");
    let removed = mix.status();
    let unknown = mix.start_output("nobody");
    mix.shutdown();

    assert_eq!(stopped.outputs.len(), 1, "a stopped output is still listed");
    assert_eq!(stopped.outputs[0].state, OutputState::Stopped);
    assert_eq!(built_while_stopped, 0, "nothing is left attached");
    assert_eq!(kept.outputs.len(), 1);
    assert!(!kept.outputs[0].enabled, "the runtime store keeps it stopped");
    assert_eq!(kept.outputs[0].uri, uri, "the address and key are kept");
    assert_eq!(started.outputs.len(), 1);
    assert_ne!(started.outputs[0].state, OutputState::Stopped);
    assert!(removed.outputs.is_empty(), "removing a stopped output forgets it");
    assert!(unknown.is_err());
}

/// An output configured stopped is listed and never built.
#[tokio::test(flavor = "multi_thread")]
async fn an_output_configured_stopped_is_not_built_at_start() {
    let _ = gst::init();
    let mut cfg = programme_config(crate::config::Accel::Software);
    let mut out = OutputConfig::bare("archive", "rtmp://127.0.0.1:1/live/key");
    out.enabled = false;
    cfg.outputs.push(out);
    let (mut mix, _handle, _rx, _bus) = Mixer::build(cfg).expect("mixer builds");
    mix.start().expect("the programme starts");
    let built = mix.outputs.len();
    let status = mix.status();
    mix.shutdown();
    assert_eq!(built, 0, "nothing is built for a stopped output");
    assert_eq!(status.outputs[0].state, OutputState::Stopped);
}
