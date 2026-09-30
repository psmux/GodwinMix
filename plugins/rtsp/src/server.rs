//! The RTSP server: one port, one mount, one shared media that every client
//! watches, on a GLib main loop of its own.
//!
//! The media is built when the first client asks for it (`DESCRIBE`) and
//! taken down after the last one leaves, so the only work done for nobody is
//! the demux in `ingest.rs`. Every client of the mount shares one
//! packetiser, whatever transport it asked for: RTP over UDP, or interleaved
//! in the RTSP connection for a client behind a firewall.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_rtsp_server as rtsp;
use gstreamer_rtsp_server::prelude::*;

use crate::feed::{Feed, Kind, Track};
use crate::settings::Settings;

pub struct Server {
    server: rtsp::RTSPServer,
    main_loop: glib::MainLoop,
    ctx: glib::MainContext,
    source: Option<glib::SourceId>,
    thread: Option<std::thread::JoinHandle<()>>,
    clients: Arc<AtomicU64>,
    mounted: AtomicBool,
    path: String,
}

impl Server {
    /// Listen now, so a port somebody else holds is said at once.
    pub fn start(s: &Settings) -> Result<Server, String> {
        let ctx = glib::MainContext::new();
        let server = rtsp::RTSPServer::new();
        server.set_address(&s.bind);
        server.set_service(&s.port.to_string());
        let source = server.attach(Some(&ctx)).map_err(|e| {
            format!("could not listen on {}:{}: {e}. Another program may be using that port; choose another in the output's settings.", s.bind, s.port)
        })?;
        let clients = Arc::new(AtomicU64::new(0));
        let count = clients.clone();
        server.connect_client_connected(move |_, client| {
            count.fetch_add(1, Ordering::Relaxed);
            let gone = count.clone();
            client.connect_closed(move |_| {
                gone.fetch_sub(1, Ordering::Relaxed);
            });
        });
        let main_loop = glib::MainLoop::new(Some(&ctx), false);
        let (run, owner) = (main_loop.clone(), ctx.clone());
        let thread = std::thread::Builder::new()
            .name("gmx-rtsp-server".into())
            .spawn(move || {
                let _ = owner.with_thread_default(|| run.run());
            })
            .map_err(|e| format!("could not start the RTSP server thread: {e}"))?;
        Ok(Server { server, main_loop, ctx, source: Some(source), thread: Some(thread), clients, mounted: AtomicBool::new(false), path: s.path.clone() })
    }

    /// Put the programme at the path, once the feed knows what it carries.
    pub fn mount(&self, feed: &Arc<Feed>) -> Result<(), String> {
        if self.mounted.swap(true, Ordering::Relaxed) {
            return Ok(());
        }
        let launch = launch(feed.track(Kind::Video), feed.track(Kind::Audio)).ok_or("the programme carries no stream RTSP here can send")?;
        let factory = rtsp::RTSPMediaFactory::new();
        factory.set_launch(&launch);
        factory.set_shared(true);
        let weak = Arc::downgrade(feed);
        factory.connect_media_configure(move |_, media| {
            let (Some(feed), Ok(bin)) = (weak.upgrade(), media.element().downcast::<gst::Bin>()) else { return };
            let base = crate::feed::Base::default();
            for (name, kind) in [("v", Kind::Video), ("a", Kind::Audio)] {
                if let Some(src) = bin.by_name(name).and_then(|e| e.downcast::<gstreamer_app::AppSrc>().ok()) {
                    feed.attach(kind, src, base.clone());
                }
            }
        });
        let mounts = self.server.mount_points().ok_or("the RTSP server has no mount table")?;
        mounts.add_factory(&self.path, factory);
        Ok(())
    }

    pub fn clients(&self) -> u64 {
        self.clients.load(Ordering::Relaxed)
    }

    pub fn mounted(&self) -> bool {
        self.mounted.load(Ordering::Relaxed)
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(m) = self.server.mount_points() {
            m.remove_factory(&self.path);
        }
        if let Some(s) = self.source.take() {
            // Destroyed through the context, which does not panic when the
            // source is already gone.
            if let Some(found) = self.ctx.find_source_by_id(&s) {
                found.destroy();
            }
        }
        self.main_loop.quit();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// The media each client session plays: an appsrc per stream, the parser
/// and the payloader. `None` when there is nothing to send.
pub fn launch(video: Option<Track>, audio: Option<Track>) -> Option<String> {
    let src = "is-live=true format=time do-timestamp=false block=false max-bytes=8000000";
    let mut parts = Vec::new();
    for (name, track) in [("v", video), ("a", audio)] {
        let Some(t) = track else { continue };
        let n = parts.len();
        let every = if t.pay == "rtph264pay" || t.pay == "rtph265pay" { " config-interval=-1" } else { "" };
        parts.push(format!("appsrc name={name} {src} ! queue ! {}{every} ! {} name=pay{n} pt={}{every}", t.parse, t.pay, 96 + n));
    }
    (!parts.is_empty()).then(|| format!("( {} )", parts.join(" ")))
}
