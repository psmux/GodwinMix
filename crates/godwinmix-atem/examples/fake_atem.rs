//! The test suite's fake ATEM as a program, so the plugin and the camera
//! window can be tried without a switcher.
//!
//! ```sh
//! cargo run -p godwinmix-atem --example fake_atem           # 127.0.0.1:9910
//! cargo run -p godwinmix-atem --example fake_atem -- 9911   # a second one
//! ```
//!
//! It answers on loopback only, keeps the state a cut or a DSK changes, and
//! prints each command it takes with the bytes of its body.

#[path = "../tests/fake/mod.rs"]
mod fake;

use std::time::Duration;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let port = std::env::args().nth(1).and_then(|p| p.parse().ok()).unwrap_or(9910);
    let f = fake::Fake::start(port, 0).await;
    eprintln!("fake ATEM on udp {}. Point the atem plugin at it: connect {{id: \"atem-demo\", address: \"{}\"}}.", f.addr, f.addr);
    let mut shown = 0;
    loop {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let blocks = f.blocks();
        for b in &blocks[shown..] {
            let name = b.get(4..8).map(String::from_utf8_lossy).unwrap_or_default();
            eprintln!("took {name} {:02x?}", b.get(8..).unwrap_or_default());
        }
        shown = blocks.len();
    }
}
