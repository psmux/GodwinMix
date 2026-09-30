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
pub mod ring;
mod time;

#[cfg(unix)]
mod link;
#[cfg(unix)]
mod publisher;
#[cfg(unix)]
pub mod shm;
#[cfg(unix)]
mod subscriber;

#[cfg(feature = "gst")]
pub mod gst;

pub use error::Error;
pub use format::{Format, Layout};
pub use name::BusName;
pub use registry::Registry;
pub use time::monotonic_ns;

#[cfg(unix)]
pub use publisher::{Publisher, PublisherOptions, PublisherStats, ReaderStats};
#[cfg(unix)]
pub use subscriber::{Frame, Subscriber};
