//! A gate for the restreamer's tests: a listener that hands every tag a
//! publisher sends to a function, and refuses any stream name but one when
//! told to.

use std::sync::Arc;

use crate::channels::split_query;
use crate::media_tag::MediaTag;
use crate::rtmp::{Gate, Inlet, Kick, Server};

/// What becomes of one tag. `None` means the publisher left.
pub type OnTag = Arc<dyn Fn(Option<MediaTag>) + Send + Sync>;

struct Keep {
    /// The one stream name let in, or any when `None`.
    key: Option<String>,
    on_tag: OnTag,
}

struct Handing(OnTag);

impl Inlet for Handing {
    fn tag(&mut self, tag: MediaTag) {
        (self.0)(Some(tag));
    }
}

impl Drop for Handing {
    fn drop(&mut self) {
        (self.0)(None);
    }
}

impl Gate for Keep {
    fn admit(&self, _app: &str, stream: &str, _peer: &str, _: Kick) -> Result<Box<dyn Inlet>, String> {
        let (stream, _) = split_query(stream);
        match &self.key {
            Some(key) if key != stream => Err("the stream key is not one this server knows".into()),
            _ => Ok(Box::new(Handing(self.on_tag.clone()))),
        }
    }

    fn note(&self, _: String) {}
}

/// A listener on `port` (0 for any) that gives every tag to `on_tag`.
pub fn listen(port: u16, key: Option<&str>, on_tag: OnTag) -> Server {
    let gate = Arc::new(Keep { key: key.map(str::to_string), on_tag });
    Server::bind("127.0.0.1", port, gate).expect("bind a loopback listener")
}
