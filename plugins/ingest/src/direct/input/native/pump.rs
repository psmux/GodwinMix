//! The socket thread of one group: receive a batch, read the tables once
//! for every show (the udp plugin's `Filter`, which also counts continuity
//! errors and stuffing for the whole group), and hand each show its own
//! program's packets.
//!
//! Nothing here waits on a show. It ends when the last show has left, and
//! the socket closes with it.

use std::sync::atomic::Ordering::Relaxed;
use std::sync::mpsc::TrySendError;
use std::sync::Arc;

use gmx_udp::counters::{Catalog, Counters};
use gmx_udp::recv::probe::RESTART_MS;
use gmx_udp::ts::filter::Filter;
use gmx_udp::ts::plan::Choice;
use gmx_udp::ts::rtp::{classify, Datagram, Sequence};
use gmx_udp::ts::{PACKET, SYNC};

use super::group::{Group, Item, Subs};
use super::recv::Batch;
use super::route::Routes;
use super::socket;

/// A program number no feed uses, so the filter only reads the tables and
/// counts, and copies nothing.
const NONE: u16 = 0xFFFF;

pub fn run(group: Arc<Group>, buffer: usize) {
    let mut filter = Filter::new(Choice { program: NONE, pids: Vec::new() });
    let (mut seq, mut batch, mut routes, mut scratch) = (Sequence::default(), Batch::default(), Routes::default(), Vec::new());
    let mut sock = None;
    let mut publish = false;
    loop {
        if closing(&group) {
            return;
        }
        let Some(s) = sock.as_ref() else {
            match socket::open(&gmx_udp::address::Endpoint { host: group.key.host.clone(), port: group.key.port, source: group.key.source.clone() }, &group.key.interface, buffer) {
                Ok((s, got)) => {
                    group.buffer.store(got, Relaxed);
                    group.set_error(None);
                    sock = Some(s);
                }
                Err(e) => {
                    group.set_error(Some(e));
                    std::thread::sleep(socket::WAIT * 4);
                }
            }
            continue;
        };
        let n = match batch.fill(s) {
            Ok(n) => n,
            Err(e) => {
                group.set_error(Some(format!("{} stopped receiving: {e}", group.address)));
                sock = None;
                continue;
            }
        };
        let counters = &*group.counters;
        let mut subs = group.subs();
        for i in 0..n {
            let d = batch.get(i);
            if counters.arrived(d.len()).is_some_and(|gap| gap > RESTART_MS) {
                filter.resumed();
                seq.reset();
                Counters::add(&counters.resumed, 1);
                subs.list.iter().for_each(|x| drop(x.tx.try_send(Item::Resumed)));
            }
            let packets = match classify(d) {
                Datagram::Ts(p) => p,
                Datagram::Rtp { seq: at, payload } => {
                    Counters::add(&counters.rtp_lost, seq.lost_before(at));
                    payload
                }
                Datagram::Junk => {
                    Counters::add(&counters.malformed, 1);
                    continue;
                }
            };
            scratch.clear();
            filter.feed(packets, counters, &mut scratch);
            if std::mem::take(&mut filter.changed) || std::mem::take(&mut subs.changed) {
                routes.build(filter.programs(), &mut subs.list);
                publish = true;
            }
            for p in packets.chunks_exact(PACKET).filter(|p| p[0] == SYNC) {
                for &to in routes.of((u16::from(p[1] & 0x1F) << 8) | u16::from(p[2])) {
                    subs.list[usize::from(to)].buf.extend_from_slice(p);
                }
            }
        }
        if std::mem::take(&mut subs.changed) {
            routes.build(filter.programs(), &mut subs.list);
        }
        deliver(&mut subs);
        drop(subs);
        if publish {
            if let Ok(mut c) = group.catalog.try_lock() {
                *c = Catalog { programs: filter.programs().to_vec(), chosen: None, problem: None };
                publish = false;
            }
        }
    }
}

/// Hand each show this batch, or count it lost if its queue is full.
fn deliver(subs: &mut Subs) {
    for s in subs.list.iter_mut().filter(|s| !s.buf.is_empty()) {
        let packets = std::mem::take(&mut s.buf);
        let n = (packets.len() / PACKET) as u64;
        if let Err(TrySendError::Full(_)) = s.tx.try_send(Item::Packets(packets)) {
            s.dropped.fetch_add(n, Relaxed);
        }
    }
}

/// True when nobody is left, and then nobody can join: a show that comes
/// later opens the group again.
fn closing(group: &Group) -> bool {
    let mut s = group.subs();
    s.closed |= s.list.is_empty();
    s.closed
}
