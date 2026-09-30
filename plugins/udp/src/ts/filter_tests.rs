use super::*;
use crate::ts::tables::{build_pat, build_pmt, parse_pat, parse_pmt, Stream};
use crate::ts::packetize;
use crate::ts::tests::packet;

/// A PAT built by hand, because `build_pat` only makes one program.
fn pat(programs: &[(u16, u16)]) -> Vec<u8> {
    let len = 5 + 4 * programs.len() + 4;
    let mut s = vec![0x00, 0xB0, len as u8, 0, 1, 0xC1, 0, 0];
    for (n, pid) in programs {
        s.extend_from_slice(&n.to_be_bytes());
        s.extend_from_slice(&(0xE000 | pid).to_be_bytes());
    }
    let crc = crate::ts::crc32(&s);
    s.extend_from_slice(&crc.to_be_bytes());
    s
}

fn pmt(number: u16, pids: &[(u16, u8)]) -> Vec<u8> {
    let p = Program { number, pmt_pid: 0, pcr_pid: pids[0].0, ..Default::default() };
    let streams: Vec<Stream> =
        pids.iter().map(|&(pid, t)| Stream { pid, stream_type: t, info: vec![] }).collect();
    build_pmt(&p, 0, &streams.iter().collect::<Vec<_>>())
}

/// Two programs, the way a DVB multiplex carries them, with stuffing.
fn mux() -> Vec<u8> {
    let mut out = Vec::new();
    packetize(0, &pat(&[(1, 0x1001), (2, 0x1002)]), &mut 0, &mut out);
    packetize(0x1001, &pmt(1, &[(256, 0x1B), (257, 0x0F)]), &mut 0, &mut out);
    packetize(0x1002, &pmt(2, &[(512, 0x1B), (513, 0x0F), (514, 0x0F)]), &mut 0, &mut out);
    for (cc, pid) in [256u16, 512, 513, 514, 257, NULL_PID].iter().enumerate() {
        out.extend(packet(*pid, cc as u8, false, &[0xAA]));
    }
    out
}

fn pids_in(stream: &[u8]) -> Vec<u16> {
    stream.chunks(PACKET).map(|p| header(p).unwrap().pid).collect()
}

fn run(choice: Choice, input: &[u8]) -> (Filter, Vec<u8>, Counters) {
    let mut f = Filter::new(choice);
    let n = Counters::default();
    let mut out = Vec::new();
    for d in input.chunks(7 * PACKET) {
        f.feed(d, &n, &mut out);
    }
    (f, out, n)
}

#[test]
fn a_single_program_feed_goes_through_untouched_but_for_stuffing() {
    let mut input = Vec::new();
    packetize(0, &build_pat(1, 0, 1, 0x1000), &mut 0, &mut input);
    packetize(0x1000, &pmt(1, &[(256, 0x1B)]), &mut 0, &mut input);
    let clean = input.clone();
    let (mut f, out, _) = run(Choice::default(), &input);
    assert_eq!(f.plan(), &Plan::PassAll);
    assert_eq!(out, clean, "one program is known from the PAT alone, so nothing is held back");
    let n = Counters::default();
    let mut again = Vec::new();
    assert!(!f.feed(&clean, &n, &mut again), "nothing changed, so the original buffer is kept");
    input.extend(packet(NULL_PID, 0, false, &[]));
    assert!(f.feed(&input, &n, &mut Vec::new()));
    assert_eq!(n.nulls.load(std::sync::atomic::Ordering::Relaxed), 1);
}

#[test]
fn choosing_program_two_hands_on_its_tables_and_streams_and_nothing_else() {
    let input = [mux(), mux()].concat();
    let (f, out, _) = run(Choice { program: 2, pids: vec![] }, &input);
    let pids = pids_in(&out);
    assert!(!pids.iter().any(|p| [256, 257, 0x1001, NULL_PID].contains(p)), "{pids:?}");
    assert!([0, 0x1002, 512, 513, 514].iter().all(|p| pids.contains(p)), "{pids:?}");
    let pat = out.chunks(PACKET).find(|p| header(p).unwrap().pid == 0).unwrap();
    assert_eq!(parse_pat(&pat[5..5 + 3 + pat[7] as usize]), vec![(2, 0x1002)]);
    assert_eq!(f.programs().len(), 2);
}

#[test]
fn naming_pids_rewrites_the_pmt_to_list_only_those() {
    let input = [mux(), mux()].concat();
    let (_, out, _) = run(Choice { program: 0, pids: vec![512, 514] }, &input);
    let pmt = out.chunks(PACKET).find(|p| header(p).unwrap().pid == 0x1002).unwrap();
    let section = &pmt[5..5 + 3 + pmt[7] as usize];
    let (_, _, streams) = parse_pmt(section).unwrap();
    assert_eq!(streams.iter().map(|s| s.pid).collect::<Vec<_>>(), vec![512, 514]);
    assert!(!pids_in(&out).contains(&513));
}

#[test]
fn a_gap_in_the_continuity_counter_is_counted_as_loss() {
    let mut input = mux();
    for cc in [0u8, 1, 4, 5] {
        input.extend(packet(256, cc, false, &[]));
    }
    let (_, _, n) = run(Choice::default(), &input);
    // 256 carried cc 0 in `mux`, then 0 again (a repeat, allowed), 1, then 4.
    assert_eq!(n.ts_lost.load(std::sync::atomic::Ordering::Relaxed), 2);
}

fn sdt(services: &[(u16, &str)]) -> Vec<u8> {
    let mut body = vec![0, 1, 0xFF]; // original_network_id, reserved
    for (id, name) in services {
        let head = [0x48u8, (3 + 4 + name.len()) as u8, 0x01, 4];
        let desc = [&head[..], b"Test", &[name.len() as u8], name.as_bytes()].concat();
        body.extend_from_slice(&id.to_be_bytes());
        body.push(0xFC);
        body.extend_from_slice(&(0x8000 | desc.len() as u16).to_be_bytes());
        body.extend(desc);
    }
    let len = 5 + body.len() + 4;
    let mut s = vec![0x42, 0xF0 | (len >> 8) as u8, len as u8, 0, 1, 0xC1, 0, 0];
    s.extend(body);
    let crc = crate::ts::crc32(&s);
    s.extend_from_slice(&crc.to_be_bytes());
    s
}

#[test]
fn service_names_that_arrive_before_the_pat_still_name_the_programs() {
    let mut input = Vec::new();
    packetize(SDT_PID, &sdt(&[(1, "News"), (2, "Sport")]), &mut 0, &mut input);
    input.extend(mux());
    let (f, _, _) = run(Choice::default(), &input);
    let names: Vec<&str> = f.programs().iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["News", "Sport"]);
    assert_eq!(f.programs()[0].provider, "Test");
}
