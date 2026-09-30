//! The frame bus. One owner opens and decodes a device or a stream once;
//! any number of readers, in this process or in others, read the decoded
//! frames straight out of shared memory with no copy.
//!
//! ```text
//!   owner (decoder)                       readers (shows, previews)
//!   Publisher::write  ──►  ring of slots in shared memory  ◄── Subscriber::next
//!        │                                  ▲
//!        └── unix socket: hands out the region's fd, a nudge per frame,
//!            and notices when a reader dies so its leases are freed
//! ```
//!
//! A reader that falls behind skips to the newest frame; it never slows the
//! owner or another reader. The owner never waits on a reader: when every
//! slot is leased it drops the frame and counts it. A reader killed mid read
//! leaves nothing locked, because its leases live in the owner's table and
//! the owner clears them when the reader's socket closes.
//!
//! See docs/explanation/frame-bus.md for why this and not `unixfdsink` or
//! `shmsink`, and docs/reference/frame-bus.md for the surface.

mod error;
pub mod format;
pub mod header;
pub mod name;
pub mod registry;
mod time;

#[cfg(unix)]
mod claim;
#[cfg(unix)]
mod link;
#[cfg(unix)]
mod publisher;
#[cfg(unix)]
pub mod ring;
#[cfg(unix)]
pub mod shm;
#[cfg(unix)]
mod subscriber;

#[cfg(all(unix, feature = "gst"))]
pub mod gst;

pub use error::Error;
pub use format::{Format, Layout};
pub use name::BusName;
pub use registry::Registry;
pub use time::monotonic_ns;

/// Whether this build can publish and read across processes. Linux and macOS
/// can. On Windows the ring, the names and the registry build, but there is
/// no transport yet: `available` says so, and a show there decodes its
/// sources itself, which is what the bus replaces elsewhere. See
/// docs/explanation/frame-bus.md.
pub const CROSS_PROCESS: bool = cfg!(unix);

/// `Ok` where the bus works, or the reason and the fallback where it does not.
pub fn available() -> Result<(), Error> {
    if CROSS_PROCESS {
        return Ok(());
    }
    Err(Error::Unsupported(
        "the frame bus has no Windows transport yet, so frames cannot be shared between \
         processes here. Each show decodes its own sources instead; nothing needs to change"
            .into(),
    ))
}

#[cfg(unix)]
pub use claim::Claim;
#[cfg(unix)]
pub use publisher::{Publisher, PublisherOptions, PublisherStats, ReaderStats};
#[cfg(unix)]
pub use subscriber::{Frame, Subscriber};
