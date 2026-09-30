//! How many readers an owner takes, and how many frames each may hold.

#[derive(Clone, Debug)]
pub struct PublisherOptions {
    /// Readers that can be attached at once. At most 32.
    pub max_readers: usize,
    /// Frames one reader may hold at once. A reader that holds this many
    /// waits for one to be dropped; the owner never does.
    pub leases_per_reader: usize,
    /// Write a checksum of every frame into its slot, for tests and the
    /// benchmark's check pass. Costs one pass over the frame.
    pub checksum: bool,
}

impl Default for PublisherOptions {
    fn default() -> Self {
        PublisherOptions {
            max_readers: 8,
            leases_per_reader: 3,
            checksum: false,
        }
    }
}
