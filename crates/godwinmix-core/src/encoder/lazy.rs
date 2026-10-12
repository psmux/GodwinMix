//! The front of an encode chain, made on its first attach rather than at boot.
//!
//! Making an element loads its plugin, and a loaded plugin stays mapped with
//! every library it links against. The audio encoder on Ubuntu is
//! `avenc_aac`, so a core that only built its encode chain and left it at
//! NULL still carried gst-libav and FFmpeg's dependencies: on the 2026-10-11
//! ci-linux bench hiding libav took an idle core from 104 MB to 71 MB. Built
//! on the first consumer instead, a core nobody reads never loads either.
//!
//! The first attach runs on the mixer thread, which is where it always ran.
//! It costs the plugin load once, tens of milliseconds for a software
//! encoder. A hardware encoder's plugin is loaded at boot anyway, by
//! `catalogue::preload`, because its driver can bring the process down while
//! it registers and that belongs before a show rather than in one.

use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use tracing::info;

/// Makes the elements in front of the tail, head first, not yet in any bin.
pub type Build = Box<dyn FnMut() -> Result<Vec<gst::Element>> + Send>;

pub(super) struct Pending {
    pub bin: gst::Bin,
    pub build: Build,
}

impl Pending {
    /// Make the front, add it to the bin and link it to `tail`'s head. On a
    /// failure nothing is left in the bin, so the next attach can try again.
    pub fn make(&mut self, tag: &str, tail: &[gst::Element]) -> Result<Vec<gst::Element>> {
        let front = (self.build)().with_context(|| format!("making the {tag} encode chain"))?;
        let first_tail = tail.first().context("an encode chain with no tail")?;
        self.bin.add_many(&front).with_context(|| format!("adding the {tag} encode chain"))?;
        let linked = gst::Element::link_many(front.iter().chain(std::iter::once(first_tail)));
        if let Err(e) = linked {
            let _ = self.bin.remove_many(&front);
            return Err(anyhow::Error::from(e).context(format!("linking the {tag} encode chain")));
        }
        info!(chain = tag, elements = front.len(), "encode chain made for its first consumer");
        Ok(front)
    }
}
