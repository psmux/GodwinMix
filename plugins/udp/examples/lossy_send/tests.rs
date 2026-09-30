use super::*;

#[test]
fn an_rtp_header_says_mp2t_with_the_sequence_and_time() {
    let h = rtp_header(0x1234, 0xA0B0C0D0);
    assert_eq!(h, [0x80, 33, 0x12, 0x34, 0xA0, 0xB0, 0xC0, 0xD0, 0, 0x6D, 0x69, 0x78]);
}

#[test]
fn it_paces_to_the_rate_and_drops_about_what_it_was_asked() {
    let rx = UdpSocket::bind("127.0.0.1:0").unwrap();
    rx.set_read_timeout(Some(Duration::from_millis(200))).unwrap();
    let port = rx.local_addr().unwrap().port();
    let args = format!("x.ts 127.0.0.1 {port} --loss 50 --seconds 0.5 --rtp");
    let a = parse(args.split(' ').map(String::from)).unwrap();
    let data = vec![0x47u8; CHUNK * 10];
    let tx = UdpSocket::bind("127.0.0.1:0").unwrap();
    // 400 datagrams a second for half a second.
    let started = Instant::now();
    let (sent, dropped) = send(&a, &data, (400 * CHUNK) as f64, &tx).unwrap();
    assert!(started.elapsed() >= Duration::from_millis(480), "paced, not flooded");
    assert!((150..=250).contains(&(sent + dropped)), "{sent} + {dropped}");
    assert!(dropped > 40 && sent > 40, "about half dropped: {sent} sent, {dropped} dropped");
    let mut buf = [0u8; 2048];
    let (n, _) = rx.recv_from(&mut buf).unwrap();
    assert_eq!((n, buf[1], buf[12]), (12 + CHUNK, 33, 0x47));
}
