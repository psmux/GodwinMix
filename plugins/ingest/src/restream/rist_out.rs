//! RIST (the Simple Profile): MPEG-TS from `crate::tsmux` in RTP, with the
//! retransmission RIST adds, by GStreamer's `ristsink`.
//!
//! ```text
//!   tsmux ──► appsrc(video/mpegts) ──► tsparse ──► rtpmp2tpay ──► ristsink
//! ```
//!
//! GStreamer here for the transport, as for SRT: RIST is RTP and RTCP with
//! NACKs on two ports, which `ristsink` already does well. `tsparse` stamps
//! the bytes from their PCR so the payloader's RTP clock is the stream's.
//! `rist://10.0.0.9:5004`; the port must be even, as RIST wants, and RTCP
//! goes to the one above. `?buffer=<ms>` is the retransmission buffer.

use gmx_netkit::pipe::Pipe;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSrc;

use crate::media_tag::MediaTag;
use crate::tsmux::Muxer;

use super::link::{Failure, Link};
use super::target::Target;
use super::udp_out::{host_port, query};

/// What `ristsink` may hold before the far end counts as slow.
const SLACK: u64 = 4 * 1024 * 1024;

pub struct RistLink {
    pipe: Pipe,
    src: AppSrc,
    muxer: Muxer,
    buf: Vec<u8>,
    name: String,
}

impl RistLink {
    pub fn dial(target: &Target) -> Result<RistLink, Failure> {
        gmx_netkit::init().map_err(Failure::Refused)?;
        gmx_netkit::elements::require(&["appsrc", "tsparse", "rtpmp2tpay", "ristsink"]).map_err(Failure::Refused)?;
        let hp = host_port(&target.url).map_err(Failure::Refused)?;
        let (host, port) = hp.rsplit_once(':').unwrap_or((&hp, ""));
        let port: u16 = port.parse().map_err(|_| Failure::Refused(format!("'{port}' is not a port")))?;
        if port % 2 == 1 {
            return Err(Failure::Refused(format!("RIST sends on an even port and RTCP on the one above; {port} is odd. Use {}.", port - 1)));
        }
        let buffer = query(&target.url).into_iter().find(|(k, _)| k == "buffer").and_then(|(_, v)| v.parse::<u32>().ok()).unwrap_or(1000);
        let host = host.trim_matches(['[', ']']).replace('"', "");
        let line = format!(
            "appsrc name=in is-live=true format=bytes caps=video/mpegts,systemstream=true,packetsize=188 \
             ! tsparse set-timestamps=true ! rtpmp2tpay ! ristsink address=\"{host}\" port={port} sender-buffer={buffer}"
        );
        let mut pipe = Pipe::launch(&line).map_err(Failure::Refused)?;
        let src: AppSrc = pipe
            .by_name("in")
            .and_then(|e| e.downcast().ok())
            .ok_or_else(|| Failure::Refused("the RIST pipeline has no appsrc".into()))?;
        let name = target.name();
        pipe.play(None).map_err(|e| Failure::Lost(format!("{name}: {e}")))?;
        Ok(RistLink { pipe, src, muxer: Muxer::new(), buf: Vec::with_capacity(64 * 1024), name })
    }
}

impl Link for RistLink {
    fn send(&mut self, tag: &MediaTag, timestamp_ms: u32) -> Result<usize, Failure> {
        if let Some(e) = self.pipe.failure() {
            return Err(Failure::Lost(format!("{} failed: {e}", self.name)));
        }
        self.muxer.tag(tag, timestamp_ms, &mut self.buf);
        if self.buf.is_empty() || self.src.current_level_bytes() > SLACK {
            // Over the slack the bytes go; the muxer's counters say so to
            // the receiver, which is what a lost packet on RIST looks like.
            self.buf.clear();
            return Ok(0);
        }
        let bytes = std::mem::replace(&mut self.buf, Vec::with_capacity(64 * 1024));
        let n = bytes.len();
        self.src
            .push_buffer(gst::Buffer::from_mut_slice(bytes))
            .map_err(|e| Failure::Lost(format!("{} stopped taking the stream ({e:?})", self.name)))?;
        Ok(n)
    }

    fn poll(&mut self) -> Result<(), Failure> {
        match self.pipe.failure() {
            Some(e) => Err(Failure::Lost(format!("{} failed: {e}", self.name))),
            None => Ok(()),
        }
    }

    fn close(&mut self) {
        let _ = self.src.end_of_stream();
        let _ = self.pipe.pipeline().set_state(gst::State::Null);
    }
}
