//! Anonymous shared memory, mapped once and handed to readers as a handle.
//!
//! Linux uses `memfd_create`. macOS has no memfd, so it opens a POSIX shared
//! memory object under a random name and unlinks it at once: from then on the
//! only way to reach it is the descriptor, exactly as with a memfd. Either
//! way nothing is left in the filesystem when every process lets go.
//!
//! The owner maps the whole region read and write. A reader maps the header
//! read and write (it writes its leases there) and the frame data read only,
//! so a reader with a bug cannot scribble on a picture another show is using.

mod os;
mod region;

pub use os::page_size;
pub use region::Region;
