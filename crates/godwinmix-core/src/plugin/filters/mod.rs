//! The filters the core ships with. Tier 0: compiled in, registered behind the
//! same trait a third party filter uses.

pub mod audio;
pub mod chroma;
pub mod colour;
#[cfg(feature = "matte")]
pub mod matte;
