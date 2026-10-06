//! The pipeline that turns a player's FLV tags into MPEG-TS on its socket.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gmx_netkit::pipe::Pipe;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::{AppSink, AppSinkCallbacks, AppSrc};

use super::gate::CapsGate;
use crate::srt::ffi::{Lib, Socket};

/// The muxing pipeline, with a branch for the sound only when there is any,
/// and its muxer held until `kinds` streams have caps (`gate.rs`).
pub fn launch(lib: &'static Lib, sock: Socket, audio: bool, kinds: usize, gone: Arc<AtomicBool>) -> Result<(Pipe, AppSrc, Arc<CapsGate>), String> {
    let sound = if audio { "d.audio ! queue ! aacparse ! mux. " } else { "" };
    let description = format!(
        "appsrc name=in is-live=true format=bytes caps=video/x-flv ! flvdemux name=d \
         {sound}mpegtsmux name=mux alignment=7 ! appsink name=out sync=false"
    );
    let mut pipe = Pipe::launch(&description)?;
    let mux = pipe.by_name("mux").ok_or("no muxer")?;
    let caps = CapsGate::hold(&mux, kinds);
    crate::restream::ts_video::video_by_codec(pipe.pipeline());
    let src: AppSrc = pipe.by_name("in").and_then(|e| e.downcast().ok()).ok_or("no appsrc")?;
    let out: AppSink = pipe.by_name("out").and_then(|e| e.downcast().ok()).ok_or("no appsink")?;
    let callbacks = AppSinkCallbacks::builder().new_sample(move |s| {
        let sample = s.pull_sample().map_err(|_| gst::FlowError::Eos)?;
        let map = sample.buffer().and_then(|b| b.map_readable().ok()).ok_or(gst::FlowError::Error)?;
        for chunk in map.chunks(1316) {
            if lib.send(sock, chunk).is_err() {
                gone.store(true, Ordering::Relaxed);
                return Err(gst::FlowError::Eos);
            }
        }
        Ok(gst::FlowSuccess::Ok)
    });
    out.set_callbacks(callbacks.build());
    pipe.play(None)?;
    let _ = src.push_buffer(gst::Buffer::from_mut_slice(crate::flv::header()));
    Ok((pipe, src, caps))
}
