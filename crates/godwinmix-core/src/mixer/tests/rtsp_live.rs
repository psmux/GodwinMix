//! An RTSP camera through a real programme: it goes live and stays live.
//!
//! The 0.3.1 report: an `rtspt://` camera went live and was judged stalled
//! about six seconds later, every time, on a healthy network. `rtspt://` fell
//! through to the clip kind, and on any RTSP scheme `rtspsrc` stamps frames
//! with the programme's running time, which the aligner then added again.
//! The camera here is served by gst-rtsp-server on loopback over TCP, which
//! is what every machine can run; the programme is left up for a few
//! seconds first, because a programme at zero hid the fault.

use super::*;
use gstreamer_rtsp_server as rtsp;
use gstreamer_rtsp_server::prelude::*;
use gstreamer::glib;

/// One RTSP mount, `/cam`, H.264 and AAC, on a main loop of its own.
struct Camera {
    main_loop: glib::MainLoop,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Camera {
    fn serve(port: u16) -> Option<Camera> {
        let _ = gst::init();
        if !["x264enc", "avenc_aac", "rtph264pay", "rtpmp4gpay"].iter().all(|f| gst::ElementFactory::find(f).is_some()) {
            println!("skipping: this GStreamer cannot encode the test camera");
            return None;
        }
        let ctx = glib::MainContext::new();
        let server = rtsp::RTSPServer::new();
        server.set_address("127.0.0.1");
        server.set_service(&port.to_string());
        let factory = rtsp::RTSPMediaFactory::new();
        factory.set_launch(
            "( videotestsrc is-live=true ! video/x-raw,width=320,height=180,framerate=25/1 ! x264enc tune=zerolatency \
             key-int-max=25 ! rtph264pay name=pay0 pt=96 audiotestsrc is-live=true ! avenc_aac ! rtpmp4gpay name=pay1 pt=97 )",
        );
        factory.set_shared(true);
        server.mount_points()?.add_factory("/cam", factory);
        server.attach(Some(&ctx)).ok()?;
        let main_loop = glib::MainLoop::new(Some(&ctx), false);
        let ml = main_loop.clone();
        let thread = std::thread::spawn(move || ml.run());
        Some(Camera { main_loop, thread: Some(thread) })
    }
}

impl Drop for Camera {
    fn drop(&mut self) {
        self.main_loop.quit();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// What a run saw once the camera was first live.
#[derive(Default, Debug)]
struct Seen {
    states_after_live: Vec<SourceState>,
    restarted: bool,
    caught_up_ms: i64,
}

/// The mixer loop by hand, ticks included, for `secs`.
async fn run(mix: &mut Mixer, cmds: &mut mpsc::Receiver<Command>, bus: &mut mpsc::Receiver<BusEvent>, secs: u64) -> Seen {
    let id: SourceId = "cam".into();
    let mut seen = Seen::default();
    let until = Instant::now() + Duration::from_secs(secs);
    let mut next_tick = Instant::now();
    while Instant::now() < until {
        while let Ok(ev) = bus.try_recv() {
            let _ = mix.handle(Command::Bus(ev));
        }
        while let Ok(cmd) = cmds.try_recv() {
            seen.restarted |= matches!(cmd, Command::RetrySource(..) | Command::RestartSource(_));
            let _ = mix.handle(cmd);
        }
        if Instant::now() >= next_tick {
            mix.tick();
            next_tick = Instant::now() + TICK;
        }
        let slot = mix.sources.iter().find(|s| s.input.id == id);
        let state = slot.map(|s| s.input.observed_state());
        if state == Some(SourceState::Live) || !seen.states_after_live.is_empty() {
            seen.states_after_live.extend(state.filter(|s| seen.states_after_live.last() != Some(s)));
        }
        seen.caught_up_ms = slot.and_then(|s| s.aligner.as_ref()).map_or(0, |a| a.catch.total() / 1_000_000);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    seen
}

async fn stays_live(uri: &str, params: &str) {
    let _ = gst::init();
    let (mut mix, _handle, mut cmds, mut bus) = Mixer::build(programme_config(crate::config::Accel::Software)).expect("mixer builds");
    mix.start().expect("the programme starts");
    // Long enough that a frame placed the programme's running time in the
    // future is held for longer than the stall limit.
    run(&mut mix, &mut cmds, &mut bus, 5).await;
    let src: SourceConfig = toml::from_str(&format!("id = \"cam\"\nuri = \"{uri}\"\n{params}")).unwrap();
    mix.add_source(&src, None).expect("the camera is added");
    assert_eq!(mix.sources.iter().find(|s| s.input.id == "cam").map(|s| s.input.type_id()), Some("hls/source".to_string()));
    mix.take(Some("cam".into()), None).unwrap();
    let seen = run(&mut mix, &mut cmds, &mut bus, 16).await;
    mix.shutdown();

    assert_eq!(seen.states_after_live, vec![SourceState::Live], "{uri} left live: {seen:?}");
    assert!(!seen.restarted, "{uri} was restarted: {seen:?}");
    assert!(seen.caught_up_ms < 1000, "{uri} was placed {} ms in the future and pulled back", seen.caught_up_ms);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_rtspt_camera_goes_live_and_stays_live() {
    let Some(_cam) = Camera::serve(20330) else { return };
    stays_live("rtspt://127.0.0.1:20330/cam", "").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_rtsp_camera_told_tcp_goes_live_and_stays_live() {
    let Some(_cam) = Camera::serve(20331) else { return };
    stays_live("rtsp://127.0.0.1:20331/cam", "params = { transport = \"tcp\" }").await;
}

/// Plain `rtsp://`, which tries UDP first. Only where `CI` is set: `rtspsrc`
/// binds its client RTP ports on every interface, which on a desktop asks the
/// firewall, and some VPN drivers drop loopback UDP altogether.
#[tokio::test(flavor = "multi_thread")]
async fn an_rtsp_camera_over_udp_goes_live_and_stays_live() {
    if std::env::var_os("CI").is_none() {
        println!("skipping: RTSP over UDP runs only where CI is set");
        return;
    }
    let Some(_cam) = Camera::serve(20332) else { return };
    stays_live("rtsp://127.0.0.1:20332/cam", "params = { transport = \"udp\" }").await;
}
