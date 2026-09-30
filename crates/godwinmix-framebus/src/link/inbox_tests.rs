use super::*;
use std::os::unix::net::UnixStream;

#[test]
fn records_and_descriptors_arrive_in_order_whatever_the_reads() {
    let (a, b) = UnixStream::pair().unwrap();
    let region = crate::shm::Region::create(4096).unwrap();
    let msg = RegionMsg {
        reader: 3,
        header_len: 4096,
        total_len: 4096,
        owner_pid: 7,
    };
    send(a.as_raw_fd(), &[NUDGE, NUDGE], None, true).unwrap();
    send_region(a.as_raw_fd(), &msg, region.fd()).unwrap();
    write_all(a.as_raw_fd(), &join_record(42)).unwrap();
    let mut inbox = Inbox::default();
    let mut got = vec![];
    while got.len() < 4 {
        wait_readable(b.as_raw_fd(), 1000).unwrap();
        assert!(inbox.fill(b.as_raw_fd()).unwrap());
        while let Some(e) = inbox.pop().unwrap() {
            got.push(e);
        }
    }
    assert!(matches!(got[..2], [Event::Nudge, Event::Nudge]));
    assert!(matches!(&got[2], Event::Region(m, _) if *m == msg));
    assert!(matches!(got[3], Event::Join(42)));
    drop(a);
    wait_readable(b.as_raw_fd(), 1000).unwrap();
    assert!(
        !inbox.fill(b.as_raw_fd()).unwrap(),
        "a closed socket reads as closed"
    );
}
