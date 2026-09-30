//! The link over a real loopback socket: two shows, one governor.

use super::client::Link;
use super::serve::Host;
use super::*;
use godwinmix_govern::{Admit, Governor, GovernorConfig, Profile, Remote};
use godwinmix_protocol::rendition::Cost;
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Default)]
struct Book {
    hellos: Mutex<Vec<String>>,
    gone: Mutex<Vec<String>>,
    on_air: Mutex<Vec<(String, bool)>>,
}

struct TestHost {
    governor: Governor,
    book: Arc<Book>,
}

impl Host for TestHost {
    fn hello(&self, hello: &Hello) -> bool {
        self.book.hellos.lock().push(hello.show.clone());
        hello.secret == "right"
    }
    fn on_air(&self, show: &str, on: bool) {
        self.book.on_air.lock().push((show.into(), on));
    }
    fn gone(&self, show: &str, _pid: u32) {
        self.book.gone.lock().push(show.into());
    }
    fn governor(&self) -> Governor {
        self.governor.clone()
    }
}

fn hello(show: &str, secret: &str) -> Hello {
    Hello { show: show.into(), addr: "127.0.0.1:1".parse().unwrap(), secret: secret.into(), pid: 1 }
}

fn cpu(m: u32) -> Cost {
    Cost { cpu_millicores: m, ..Cost::default() }
}

/// Wait for something the station does on its own task, for at most two
/// seconds, so a broken link fails the test rather than hanging it.
fn until(what: &str, test: impl Fn() -> bool) {
    let start = Instant::now();
    while !test() {
        assert!(start.elapsed() < Duration::from_secs(2), "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_shows_ask_one_governor_and_a_show_that_goes_gives_its_share_back() {
    let governor = Governor::with_machine(GovernorConfig::default(), Profile::uncalibrated(), 8, 16_384);
    let book = Arc::new(Book::default());
    let addr = listen(Arc::new(TestHost { governor: governor.clone(), book: book.clone() })).await.unwrap();

    let result = tokio::task::spawn_blocking(move || {
        let a = Link::connect(addr, &hello("a", "right"), Box::new(|| {})).unwrap();
        let b = Link::connect(addr, &hello("b", "right"), Box::new(|| {})).unwrap();
        let show_a = Governor::with_machine(GovernorConfig::default(), Profile::uncalibrated(), 8, 16_384);
        show_a.set_remote(a.clone());
        let show_b = Governor::with_machine(GovernorConfig::default(), Profile::uncalibrated(), 8, 16_384);
        show_b.set_remote(b.clone());

        let held = show_a.admit(cpu(5000), "show a").granted().expect("the first show fits");
        until("the station to book it", || governor.held().len() == 1);
        assert!(matches!(show_b.admit(cpu(5000), "show b"), Admit::Refused { .. }), "one machine");

        a.on_air(true);
        until("on air to arrive", || book.on_air.lock().len() == 1);

        // Show a dies with its ticket still held: the link closes and the
        // station gives the share back without being told.
        std::mem::forget(held);
        a.close();
        until("the station to see show a go", || book.gone.lock().contains(&"a".to_string()));
        assert!(governor.held().is_empty());
        let t = show_b.admit(cpu(5000), "show b").granted().expect("room now");
        drop(t);
        until("the release to arrive", || governor.held().is_empty());
        assert!(b.ask(&godwinmix_govern::Ask { what: "x".into(), cost: cpu(1), kind: Default::default(), device: None, encode: None }).is_some());
    })
    .await;
    result.unwrap();
}

#[tokio::test]
async fn a_hello_with_the_wrong_secret_is_turned_away() {
    let governor = Governor::with_machine(GovernorConfig::default(), Profile::uncalibrated(), 8, 16_384);
    let book = Arc::new(Book::default());
    let addr = listen(Arc::new(TestHost { governor, book: book.clone() })).await.unwrap();
    let lost = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = lost.clone();
    tokio::task::spawn_blocking(move || {
        let _link = Link::connect(addr, &hello("x", "wrong"), Box::new(move || flag.store(true, std::sync::atomic::Ordering::SeqCst))).unwrap();
        until("the station to close it", || lost.load(std::sync::atomic::Ordering::SeqCst));
    })
    .await
    .unwrap();
    assert!(book.gone.lock().is_empty(), "a show that never said hello never went");
}
