//! What carries VISCA bytes to a camera and the reply back.

use super::reply::{error, Reply};
use crate::error::Result;

pub trait Link: Send {
    /// Send one message and answer what settles it: the acknowledgement or
    /// completion of a command, or the data of an inquiry.
    fn exchange(&mut self, message: &[u8], inquiry: bool) -> Result<Vec<u8>>;
    /// How the camera is named in error messages.
    fn name(&self) -> &str;
}

/// Whether a reply settles an exchange. An acknowledgement settles a command
/// (the completion of a preset recall comes seconds later and nobody waits
/// for it) but not an inquiry, which waits for its data. An empty completion
/// never settles an inquiry: on raw TCP it is the late completion of the
/// command before, since an inquiry's answer always carries data. With no
/// sequence numbers to match (`by_order`), a command waits for its own
/// acknowledgement, because an empty completion there may be the last one's.
pub fn settle(reply: Reply, inquiry: bool, by_order: bool, to: &str) -> Option<Result<Vec<u8>>> {
    match reply {
        Reply::Ack if inquiry => None,
        Reply::Done(data) if data.is_empty() && (inquiry || by_order) => None,
        Reply::Ack => Some(Ok(Vec::new())),
        Reply::Done(data) => Some(Ok(data)),
        Reply::Error(code) => Some(Err(error(to, code))),
    }
}
