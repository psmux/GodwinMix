//! The bytes on the wire: the twelve byte packet header and the command
//! blocks that follow it. Nothing here keeps state or touches a socket.
//!
//! `docs/reference/atem-protocol.md` has the same layout in prose.

mod blocks;
mod header;

pub use blocks::{block, Block, Blocks};
pub use header::{Flags, Header, HEADER_LEN, MAX_PACKET_ID};
