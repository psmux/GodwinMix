//! `ext.programme` on `/rpc`: the programme at full quality for a full
//! screen output. What the client asked for, and its frames out.
//!
//! The feed itself is `godwinmix_core::preview::feed`. A connection holds a
//! `FeedSubscription` for as long as its subscription asks, and that hold is
//! what keeps the branch on the programme's raw tee.

use super::Connection;
use axum::extract::ws::Message;
use godwinmix_core::preview::feed::FeedAsk;
use godwinmix_protocol::rpc;
use godwinmix_protocol::Ext;

/// What `ext.programme` asks the feed for, or `None` when it asks nothing.
pub fn feed_ask(ext: &Ext) -> Option<FeedAsk> {
    let programme = ext.programme.as_ref().filter(|p| p.wanted())?;
    let (width, fps) = programme.ask();
    Some(FeedAsk { width, fps })
}

impl Connection {
    /// One programme frame out, in the same 16 byte header as a mosaic frame,
    /// with the stream bit set and the layout field naming the programme.
    pub(super) async fn send_programme_frame(&mut self, jpeg: &[u8]) -> Result<(), ()> {
        if !self.sub.as_ref().is_some_and(|s| s.wants("programme.frame")) {
            return Ok(());
        }
        self.programme_no = self.programme_no.wrapping_add(1);
        let header = rpc::programme_frame_header(self.programme_no, self.clock.now_ms());
        let mut out = Vec::with_capacity(header.len() + jpeg.len());
        out.extend_from_slice(&header);
        out.extend_from_slice(jpeg);
        self.write(Message::Binary(out.into())).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_ask_reads_true_an_object_and_false() {
        let ext = |v| serde_json::from_value::<Ext>(json!({ "programme": v })).unwrap();
        assert_eq!(feed_ask(&ext(json!(true))), Some(FeedAsk { width: 0, fps: 0 }));
        assert_eq!(feed_ask(&ext(json!({ "width": 1920, "fps": 25 }))), Some(FeedAsk { width: 1920, fps: 25 }));
        assert_eq!(feed_ask(&ext(json!(false))), None);
        assert_eq!(feed_ask(&Ext::default()), None);
    }
}
