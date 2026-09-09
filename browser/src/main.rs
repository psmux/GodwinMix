//! liveboxmix-browser: a web page as a raw audio and video source.
//!
//! Chromium is embedded through CEF and run windowless. It hands back every
//! rendered frame as a BGRA buffer and every audio packet as PCM, straight from
//! the browser, with no screen grab and no capture encode in between. That is
//! the difference from `tools/browser-source.sh`: nothing lossy sits between
//! the page and the mixer.
//!
//! Output is a Matroska stream of raw I420 video and float PCM on stdout, which
//! is exactly what the mixer's `exec:` source reads:
//!
//!   liveboxmix ctl source add site \
//!     "exec:liveboxmix-browser --url https://example.com --width 1920 --height 1080 --fps 30"
//!
//! CEF's process model: the same executable is re-launched by Chromium for its
//! renderer, GPU and utility subprocesses. `execute_process` returns >= 0 in
//! those, and they must return immediately without touching anything else.
//! Everything written to stdout is stream data; all logging goes to stderr.

mod mux;

use cef::{args::Args, *};
use mux::Muxer;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

struct Opts {
    url: String,
    width: i32,
    height: i32,
    fps: i32,
    /// Stop after this many seconds; 0 runs until the process is signalled.
    seconds: u64,
    resources_dir: Option<String>,
    locales_dir: Option<String>,
    cache_dir: PathBuf,
    /// Added to every audio timestamp, in milliseconds. See `mux.rs` for the
    /// default and how it was measured.
    audio_offset_ms: i64,
    /// Watch the page for the media it is playing and report what it finds on
    /// stderr as `[browser] media {json}`. See `detect-media.js`.
    detect_media: bool,
    /// Emit the page with a real alpha channel instead of on opaque black, so
    /// the mixer can put its own picture behind it. With `--detect-media` it
    /// also stops the page's own video being painted, which is the point: the
    /// mixer decodes that video on the GPU and draws the page over the top.
    transparent: bool,
}

fn opts() -> Opts {
    let mut o = Opts {
        url: "about:blank".into(),
        width: 1280,
        height: 720,
        fps: 30,
        seconds: 0,
        resources_dir: None,
        locales_dir: None,
        audio_offset_ms: mux::DEFAULT_AUDIO_OFFSET_MS,
        // Every instance gets its own profile. CEF treats the cache directory
        // as a process singleton lock, so two sources sharing one would block
        // each other, and `initialize` would hang waiting for the lock.
        cache_dir: std::env::temp_dir().join(format!("lbx-browser-{}", std::process::id())),
        detect_media: false,
        transparent: false,
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut val = || it.next().unwrap_or_default();
        match a.as_str() {
            "--url" => o.url = val(),
            "--width" => o.width = val().parse().unwrap_or(1280),
            "--height" => o.height = val().parse().unwrap_or(720),
            "--fps" => o.fps = val().parse().unwrap_or(30),
            "--audio-offset-ms" => o.audio_offset_ms = val().parse().unwrap_or(mux::DEFAULT_AUDIO_OFFSET_MS),
            "--seconds" => o.seconds = val().parse().unwrap_or(0),
            "--resources-dir" => o.resources_dir = Some(val()),
            "--locales-dir" => o.locales_dir = Some(val()),
            "--cache-dir" => o.cache_dir = PathBuf::from(val()),
            "--detect-media" => o.detect_media = true,
            "--transparent" => o.transparent = true,
            _ => {} // Chromium's own switches pass through untouched.
        }
    }
    o
}

/// Everything the handlers share.
struct Shared {
    width: i32,
    height: i32,
    mux: Option<Arc<Muxer>>,
    frames: u64,
    audio_packets: u64,
    audio_frames: u64,
    audio_channels: i32,
    audio_rate: i32,
}

type State = Arc<Mutex<Shared>>;

wrap_render_handler! {
    struct Renderer {
        state: State,
    }

    impl RenderHandler {
        /// The size the browser lays out and paints at. This is what makes the
        /// page render sharp at canvas resolution rather than being scaled.
        fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
            let s = self.state.lock().unwrap();
            if let Some(r) = rect {
                r.x = 0;
                r.y = 0;
                r.width = s.width;
                r.height = s.height;
            }
        }

        fn screen_info(
            &self,
            _browser: Option<&mut Browser>,
            info: Option<&mut ScreenInfo>,
        ) -> ::std::os::raw::c_int {
            let s = self.state.lock().unwrap();
            if let Some(i) = info {
                i.device_scale_factor = 1.0;
                i.depth = 32;
                i.depth_per_component = 8;
                i.is_monochrome = 0;
                i.rect = Rect { x: 0, y: 0, width: s.width, height: s.height };
                i.available_rect = Rect { x: 0, y: 0, width: s.width, height: s.height };
                return 1;
            }
            0
        }

        fn on_paint(
            &self,
            _browser: Option<&mut Browser>,
            type_: PaintElementType,
            _dirty_rects: Option<&[Rect]>,
            buffer: *const u8,
            width: ::std::os::raw::c_int,
            height: ::std::os::raw::c_int,
        ) {
            // Popups (dropdowns) paint separately; only the view is the picture.
            if sys::cef_paint_element_type_t::from(type_) != sys::cef_paint_element_type_t::PET_VIEW {
                return;
            }
            let mut s = self.state.lock().unwrap();
            s.frames += 1;
            if let Some(m) = &s.mux {
                let len = (width * height * 4) as usize;
                let bgra = unsafe { std::slice::from_raw_parts(buffer, len) };
                m.video_frame(bgra);
            }
        }
    }
}

wrap_audio_handler! {
    struct Audio {
        state: State,
    }

    impl AudioHandler {
        /// Ask Chromium for the format the mixer's canvas uses, so nothing has
        /// to be resampled afterwards. Returning 1 means "use these".
        fn audio_parameters(
            &self,
            _browser: Option<&mut Browser>,
            params: Option<&mut AudioParameters>,
        ) -> ::std::os::raw::c_int {
            if let Some(p) = params {
                p.channel_layout = ChannelLayout::from(sys::cef_channel_layout_t::CEF_CHANNEL_LAYOUT_STEREO);
                p.sample_rate = 48000;
                p.frames_per_buffer = 1024;
                return 1;
            }
            0
        }

        fn on_audio_stream_started(
            &self,
            _browser: Option<&mut Browser>,
            params: Option<&AudioParameters>,
            channels: ::std::os::raw::c_int,
        ) {
            let mut s = self.state.lock().unwrap();
            s.audio_channels = channels;
            s.audio_rate = params.map(|p| p.sample_rate).unwrap_or(48000);
            eprintln!("[browser] audio started: {} ch @ {} Hz", channels, s.audio_rate);
            if let Some(m) = &s.mux {
                m.audio_stream_started();
            }
            if channels != 2 || s.audio_rate != 48000 {
                eprintln!("[browser] warning: browser did not honour the requested audio format");
            }
        }

        /// Planar float PCM: `data[ch]` points at `frames` samples for channel
        /// `ch`. Interleave, which is what the muxer's caps declare.
        fn on_audio_stream_packet(
            &self,
            _browser: Option<&mut Browser>,
            data: *mut *const f32,
            frames: ::std::os::raw::c_int,
            pts: i64,
        ) {
            let mut s = self.state.lock().unwrap();
            let ch = s.audio_channels.max(1) as usize;
            let n = frames.max(0) as usize;
            let mut out = Vec::with_capacity(n * ch * 4);
            unsafe {
                let planes = std::slice::from_raw_parts(data, ch);
                for i in 0..n {
                    for &plane in planes {
                        out.extend_from_slice(&(*plane.add(i)).to_le_bytes());
                    }
                }
            }
            s.audio_packets += 1;
            s.audio_frames += n as u64;
            if let Some(m) = &s.mux {
                m.audio_packet(&out, pts);
            }
        }

        fn on_audio_stream_error(&self, _browser: Option<&mut Browser>, message: Option<&CefString>) {
            eprintln!("[browser] audio error: {}", message.map(|m| m.to_string()).unwrap_or_default());
        }
    }
}

wrap_life_span_handler! {
    struct LifeSpan {
        state: State,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, _browser: Option<&mut Browser>) {
            eprintln!("[browser] browser created");
        }

        fn on_before_close(&self, _browser: Option<&mut Browser>) {
            eprintln!("[browser] browser closed");
            quit_message_loop();
        }
    }
}

/// Watches the page for the media it is playing. Injected on every load.
const DETECT_MEDIA_JS: &str = include_str!("detect-media.js");
/// Presses the page's own "Enable Sound" for it. See `unmute.js`.
const UNMUTE_JS: &str = include_str!("unmute.js");

/// Prefix the injected script puts on its console line, so the page's own
/// logging is not mistaken for a report.
const MEDIA_TAG: &str = "LBX_MEDIA ";

wrap_load_handler! {
    struct Load {
        state: State,
        detect_media: bool,
        transparent: bool,
    }

    impl LoadHandler {
        fn on_load_end(
            &self,
            _browser: Option<&mut Browser>,
            frame: Option<&mut Frame>,
            http_status_code: ::std::os::raw::c_int,
        ) {
            eprintln!("[browser] load finished, http {http_status_code}");
            // Re-injected per load because a navigation discards the last one.
            // The script itself is idempotent, which covers same-document
            // navigations that fire this more than once.
            // A whole page keeps its own players, and they start muted until
            // someone clicks. Nobody will, so the sidecar does. Not in
            // superimpose mode: there the mixer plays the media itself and the
            // page's copy is paused on purpose.
            if !self.transparent {
                if let Some(f) = frame.as_deref() {
                    f.execute_java_script(
                        Some(&UNMUTE_JS.into()),
                        Some(&"lbx://unmute.js".into()),
                        0,
                    );
                }
            }
            if self.detect_media {
                if let Some(f) = frame {
                    // The prelude goes in front of the script rather than in a
                    // second injection because the flag has to be set before
                    // the script reads it, and the script refuses to run twice.
                    let js = if self.transparent {
                        format!("window.__lbxHideMedia = true;\n{DETECT_MEDIA_JS}")
                    } else {
                        DETECT_MEDIA_JS.to_string()
                    };
                    f.execute_java_script(
                        Some(&js.as_str().into()),
                        Some(&"lbx://detect-media.js".into()),
                        0,
                    );
                }
            }
        }

        fn on_load_error(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            _error_code: Errorcode,
            error_text: Option<&CefString>,
            failed_url: Option<&CefString>,
        ) {
            eprintln!(
                "[browser] load error: {} ({})",
                error_text.map(|t| t.to_string()).unwrap_or_default(),
                failed_url.map(|u| u.to_string()).unwrap_or_default()
            );
        }
    }
}

wrap_display_handler! {
    struct Console;

    impl DisplayHandler {
        /// The injected detector reports here. The console is what carries it
        /// out of the renderer process; everything else on it is the page's own
        /// noise and is dropped.
        fn on_console_message(
            &self,
            _browser: Option<&mut Browser>,
            _level: LogSeverity,
            message: Option<&CefString>,
            _source: Option<&CefString>,
            _line: ::std::os::raw::c_int,
        ) -> ::std::os::raw::c_int {
            if let Some(m) = message {
                let m = m.to_string();
                if let Some(report) = m.strip_prefix(MEDIA_TAG) {
                    eprintln!("[browser] media {report}");
                    return 1; // Handled: keep it out of Chromium's own log.
                }
            }
            0
        }
    }
}

wrap_client! {
    struct SourceClient {
        state: State,
        detect_media: bool,
        transparent: bool,
    }

    impl Client {
        fn display_handler(&self) -> Option<DisplayHandler> {
            Some(Console::new())
        }

        fn render_handler(&self) -> Option<RenderHandler> {
            Some(Renderer::new(self.state.clone()))
        }
        fn audio_handler(&self) -> Option<AudioHandler> {
            Some(Audio::new(self.state.clone()))
        }
        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            Some(LifeSpan::new(self.state.clone()))
        }
        fn load_handler(&self) -> Option<LoadHandler> {
            Some(Load::new(self.state.clone(), self.detect_media, self.transparent))
        }
    }
}

wrap_browser_process_handler! {
    struct ProcessHandler {
        state: State,
        url: String,
        fps: i32,
        detect_media: bool,
        transparent: bool,
    }

    impl BrowserProcessHandler {
        /// CEF is up; create the windowless browser.
        fn on_context_initialized(&self) {
            // A null parent. The window handle type is a pointer on macOS, a
            // newtype around one (HWND) on Windows, and an integer on Linux.
            #[cfg(target_os = "macos")]
            let no_parent = std::ptr::null_mut();
            #[cfg(target_os = "windows")]
            let no_parent = cef_dll_sys::HWND(std::ptr::null_mut());
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            let no_parent = 0;
            let window_info = WindowInfo::default().set_as_windowless(no_parent);
            let settings = BrowserSettings {
                windowless_frame_rate: self.fps,
                // Nothing is painted behind the page in transparent mode, so
                // whatever the page does not cover leaves the browser with
                // alpha 0 and the mixer can put its own picture there.
                // Otherwise opaque black, which is what an ordinary source
                // wants: no alpha to carry and nothing to composite against.
                background_color: if self.transparent { 0x0000_0000 } else { 0xFF00_0000 },
                ..Default::default()
            };
            let mut client =
                SourceClient::new(self.state.clone(), self.detect_media, self.transparent);
            let url = CefString::from(self.url.as_str());
            let ok = browser_host_create_browser(
                Some(&window_info),
                Some(&mut client),
                Some(&url),
                Some(&settings),
                None,
                None,
            );
            eprintln!("[browser] create browser -> {ok}");
        }
    }
}

wrap_app! {
    struct SourceApp {
        state: State,
        url: String,
        fps: i32,
        detect_media: bool,
        transparent: bool,
    }

    impl App {
        /// Chromium switches. Applied to the browser process only, which is the
        /// one with an empty process type.
        fn on_before_command_line_processing(
            &self,
            process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            let is_browser = process_type.map(|p| p.to_string().is_empty()).unwrap_or(true);
            let Some(cl) = command_line else { return };
            if !is_browser {
                return;
            }
            for sw in [
                // Without this, Chrome's first-run flow tries to show a EULA
                // dialog from inside CefInitialize. Windowless, nobody can
                // dismiss it, and initialize() never returns. Found with gdb:
                // first_run::internal::ShowEulaDialog() parked in a RunLoop.
                "no-first-run",
                "no-default-browser-check",
                "no-sandbox",
                "disable-gpu",
                "disable-gpu-compositing",
                "disable-dev-shm-usage",
                // Render audio into a fake output device. The audio handler
                // taps the stream before the device, so it still gets every
                // sample (measured: same packet count with and without a real
                // sink), no sound leaks out of a workstation's speakers, and a
                // headless box needs no PulseAudio. Deliberately not
                // --mute-audio: muting stops the stream from being created at
                // all, and no packets ever arrive.
                "disable-audio-output",
            ] {
                cl.append_switch(Some(&CefString::from(sw)));
            }
            cl.append_switch_with_value(
                Some(&CefString::from("autoplay-policy")),
                Some(&CefString::from("no-user-gesture-required")),
            );
            // Say who we are. A page can tell a broadcast capture from a
            // viewer by the user agent and start with sound (a page that plays a
            // stream can), instead of waiting for a click nobody will make.
            cl.append_switch_with_value(
                Some(&CefString::from("user-agent-product")),
                Some(&CefString::from(
                    concat!("LiveboxMix/", env!("CARGO_PKG_VERSION")).to_string().as_str(),
                )),
            );
            // Extra Chromium switches from the operator, comma separated,
            // without the leading dashes: LBX_BROWSER_SWITCHES="enable-gpu,foo=bar".
            if let Ok(extra) = std::env::var("LBX_BROWSER_SWITCHES") {
                for sw in extra.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                    match sw.split_once('=') {
                        Some((k, v)) => cl.append_switch_with_value(
                            Some(&CefString::from(k)),
                            Some(&CefString::from(v)),
                        ),
                        None => cl.append_switch(Some(&CefString::from(sw))),
                    }
                }
            }
        }

        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(ProcessHandler::new(
                self.state.clone(),
                self.url.clone(),
                self.fps,
                self.detect_media,
                self.transparent,
            ))
        }
    }
}

wrap_task! {
    struct Quit;

    impl Task {
        fn execute(&self) {
            eprintln!("[browser] time is up, stopping");
            quit_message_loop();
        }
    }
}

static STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

extern "C" fn on_signal(_sig: libc::c_int) {
    STOP.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// macOS runs CEF out of an app bundle: the framework is loaded by hand from
/// Contents/Frameworks before the first CEF call, and AppKit's shared
/// application has to be a class that implements CefAppProtocol.
#[cfg(target_os = "macos")]
mod mac {
    use cef::application_mac::{CefAppProtocol, CrAppControlProtocol, CrAppProtocol};
    use objc2::{define_class, msg_send, rc::Retained, runtime::Bool, ClassType, MainThreadOnly};
    use objc2_app_kit::{NSApplication, NSEvent};
    use std::sync::atomic::{AtomicBool, Ordering};

    static HANDLING_SEND_EVENT: AtomicBool = AtomicBool::new(false);

    define_class!(
        #[unsafe(super(NSApplication))]
        #[thread_kind = MainThreadOnly]
        #[name = "LiveboxBrowserApplication"]
        struct Application;

        unsafe impl CrAppProtocol for Application {
            #[unsafe(method(isHandlingSendEvent))]
            fn is_handling_send_event(&self) -> Bool {
                Bool::new(HANDLING_SEND_EVENT.load(Ordering::SeqCst))
            }
        }

        unsafe impl CrAppControlProtocol for Application {
            #[unsafe(method(setHandlingSendEvent:))]
            fn set_handling_send_event(&self, handling: Bool) {
                HANDLING_SEND_EVENT.store(handling.as_bool(), Ordering::SeqCst);
            }
        }

        unsafe impl CefAppProtocol for Application {}

        impl Application {
            #[unsafe(method(sendEvent:))]
            fn send_event(&self, event: &NSEvent) {
                let was = HANDLING_SEND_EVENT.swap(true, Ordering::SeqCst);
                unsafe {
                    let _: () = msg_send![super(self), sendEvent: event];
                }
                HANDLING_SEND_EVENT.store(was, Ordering::SeqCst);
            }
        }
    );

    pub fn load_framework() -> cef::library_loader::LibraryLoader {
        let exe = std::env::current_exe().expect("own path");
        let loader = cef::library_loader::LibraryLoader::new(&exe, false);
        if !loader.load() {
            eprintln!("[browser] could not load the Chromium Embedded Framework next to {}", exe.display());
            std::process::exit(2);
        }
        loader
    }

    pub fn create_application() {
        let _app: Retained<Application> = unsafe { msg_send![Application::class(), sharedApplication] };
    }
}

fn main() {
    #[cfg(target_os = "macos")]
    let _framework = mac::load_framework();
    let args = Args::new();
    let _ = api_hash(sys::CEF_API_VERSION_LAST, 0);

    // Subprocesses (renderer, GPU, utility) return here and must do nothing else.
    let code = execute_process(Some(args.as_main_args()), None::<&mut App>, std::ptr::null_mut());
    if code >= 0 {
        std::process::exit(code);
    }

    let o = opts();
    let state: State = Arc::new(Mutex::new(Shared {
        width: o.width,
        height: o.height,
        mux: None,
        frames: 0,
        audio_packets: 0,
        audio_frames: 0,
        audio_channels: 0,
        audio_rate: 0,
    }));

    #[cfg(target_os = "macos")]
    mac::create_application();
    let mut app =
        SourceApp::new(state.clone(), o.url.clone(), o.fps, o.detect_media, o.transparent);
    let _ = std::fs::create_dir_all(&o.cache_dir);
    let cache = o.cache_dir.to_string_lossy().to_string();
    let settings = Settings {
        windowless_rendering_enabled: 1,
        no_sandbox: 1,
        root_cache_path: CefString::from(cache.as_str()),
        cache_path: CefString::from(cache.as_str()),
        log_severity: LogSeverity::from(sys::cef_log_severity_t::LOGSEVERITY_WARNING),
        resources_dir_path: CefString::from(o.resources_dir.as_deref().unwrap_or("")),
        locales_dir_path: CefString::from(o.locales_dir.as_deref().unwrap_or("")),
        ..Default::default()
    };
    let ok = initialize(Some(args.as_main_args()), Some(&settings), Some(&mut app), std::ptr::null_mut());
    if ok != 1 {
        eprintln!("[browser] cef initialize failed");
        std::process::exit(2);
    }

    // Stream goes to stdout (fd 1). Stereo 48 kHz is what audio_parameters
    // asks the browser for, so the muxer's caps match what arrives.
    let mux = match Muxer::new(
        o.width,
        o.height,
        o.fps,
        2,
        48000,
        1,
        o.audio_offset_ms,
        o.transparent,
    ) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("[browser] output pipeline failed: {e}");
            std::process::exit(3);
        }
    };
    state.lock().unwrap().mux = Some(mux.clone());
    eprintln!("[browser] rendering {} at {}x{} @ {} fps", o.url, o.width, o.height, o.fps);

    // Stop cleanly on SIGTERM and SIGINT: the mixer signals the process group
    // when a source is removed, and Chromium's helpers are in that group too.
    // quit_message_loop must run on CEF's UI thread, so a watcher posts it.
    unsafe {
        libc::signal(libc::SIGTERM, on_signal as *const () as libc::sighandler_t);
        libc::signal(libc::SIGINT, on_signal as *const () as libc::sighandler_t);
    }
    std::thread::spawn(|| {
        while !STOP.load(std::sync::atomic::Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        eprintln!("[browser] signalled, stopping");
        let mut task = Quit::new();
        post_task(ThreadId::UI, Some(&mut task));
    });
    // The reader of stdout went away: stop rather than paint into a dead pipe.
    {
        let mux = mux.clone();
        std::thread::spawn(move || {
            let why = mux.wait_for_failure();
            eprintln!("[browser] output stopped: {why}");
            let mut task = Quit::new();
            post_task(ThreadId::UI, Some(&mut task));
        });
    }

    if o.seconds > 0 {
        // quit_message_loop must run on CEF's UI thread, so post it there.
        let secs = o.seconds;
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(secs));
            let mut task = Quit::new();
            post_task(ThreadId::UI, Some(&mut task));
        });
    }

    run_message_loop();
    mux.finish();
    shutdown();
    // The profile was private to this run; leave nothing behind.
    let _ = std::fs::remove_dir_all(&o.cache_dir);

    let s = state.lock().unwrap();
    eprintln!(
        "[browser] done: paints={} audio_packets={} audio_frames={} ({} ch @ {} Hz)",
        s.frames, s.audio_packets, s.audio_frames, s.audio_channels, s.audio_rate
    );
}
