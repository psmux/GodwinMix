//! Keyed and open, by where the player is.

use super::*;
use godwinmix_core::hls::HlsParams;
use godwinmix_protocol::scope::{Token, Tokens};
use std::sync::Arc;

#[derive(Clone)]
struct Only(Arc<Tokens>);

impl Door for Only {
    fn tokens(&self) -> &Tokens {
        &self.0
    }
    fn find(&self, _: &str, _: &Request) -> Result<Arc<Stream>, Box<Response>> {
        unreachable!("admit is handed the stream")
    }
}

fn from(peer: &str, query: &str) -> Request {
    let mut req = Request::builder().uri(format!("/hls/ch/master.m3u8{query}")).body(axum::body::Body::empty()).unwrap();
    req.extensions_mut().insert(ConnectInfo(format!("{peer}:5000").parse::<SocketAddr>().unwrap()));
    req
}

fn stream(access: Access) -> Stream {
    Stream::new("ch", HlsParams { access, ..HlsParams::default() }, &"k".repeat(24))
}

#[test]
fn open_answers_loopback_and_private_networks_and_refuses_the_rest_with_why() {
    let door = Only(Arc::new(Tokens::new(vec![Token::legacy("control-secret")], false)));
    let open = stream(Access::Open);
    for local in ["127.0.0.1", "192.168.1.5", "10.0.0.7"] {
        assert!(admit(&door, &open, &from(local, "")).is_ok(), "{local}");
    }
    let refused = admit(&door, &open, &from("8.8.8.8", "")).err().expect("a public address needs the key");
    assert_eq!(refused.status(), StatusCode::FORBIDDEN);
    assert!(admit(&door, &open, &from("8.8.8.8", &format!("?key={}", "k".repeat(24)))).is_ok(), "the key still opens it");
    assert!(admit(&door, &open, &from("8.8.8.8", "?token=control-secret")).is_ok(), "and a token");
    assert!(admit(&door, &stream(Access::OpenPublic), &from("8.8.8.8", "")).is_ok(), "open_public: anyone");
    let keyed = admit(&door, &stream(Access::Keyed), &from("127.0.0.1", "")).err().expect("keyed asks loopback too");
    assert_eq!(keyed.status(), StatusCode::UNAUTHORIZED);
}
