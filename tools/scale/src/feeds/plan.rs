//! Which clip each feed sends, where to, with what impairment, and the feeds
//! list as the CSV `show.add_many` is fed from.

use super::clip::Clip;
use super::send::Feed;
use crate::net;
use std::fmt::Write as _;
use std::net::SocketAddrV4;
use std::sync::Arc;

#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Impair {
    pub loss: f64,
    pub jitter_ms: f64,
}

/// `0-9:loss=1,jitter=20` or `17:loss=0.5`: feeds by index, first to last.
pub fn impairment(spec: &str) -> Result<(usize, usize, Impair), String> {
    let bad = || format!("{spec} is not an impairment. Write it as 0-9:loss=1,jitter=20");
    let (range, what) = spec.split_once(':').ok_or_else(bad)?;
    let (a, b) = range.split_once('-').unwrap_or((range, range));
    let (a, b) = (a.parse().map_err(|_| bad())?, b.parse().map_err(|_| bad())?);
    let mut imp = Impair::default();
    for kv in what.split(',') {
        let (k, v) = kv.split_once('=').ok_or_else(bad)?;
        let v: f64 = v.parse().map_err(|_| bad())?;
        match k {
            "loss" => imp.loss = v,
            "jitter" => imp.jitter_ms = v,
            _ => return Err(bad()),
        }
    }
    Ok((a, b, imp))
}

pub struct Plan {
    pub feeds: Vec<Feed>,
    pub rows: Vec<Row>,
}

pub struct Row {
    pub name: String,
    pub input: String,
    pub program: u16,
}

/// Feed `i` sends clip `i % clips` to the i-th address after `to`, and
/// starts at a different place in it than its neighbours.
pub fn build(clips: &[Arc<Clip>], count: usize, (to, port_step): (SocketAddrV4, bool), base: Impair, special: &[(usize, usize, Impair)]) -> Plan {
    let mut plan = Plan { feeds: Vec::new(), rows: Vec::new() };
    for i in 0..count {
        let clip = Arc::clone(&clips[i % clips.len()]);
        let at = net::nth(to, i as u32, port_step);
        let imp = special.iter().rev().find(|(a, b, _)| (*a..=*b).contains(&i)).map_or(base, |s| s.2);
        let start = (i * 7919) % clip.datagrams();
        let program = clip.programs.get((i / clips.len()) % clip.programs.len().max(1)).copied().unwrap_or(1);
        let input = if at.ip().is_multicast() { format!("udp://@{at}") } else { format!("udp://@:{}", at.port()) };
        plan.rows.push(Row { name: format!("feed-{:03}", i + 1), input, program });
        let jitter_ns = (imp.jitter_ms * 1e6) as u64;
        plan.feeds.push(Feed { clip, to: at, loss: imp.loss, jitter_ns, start, seed: 0x9E37_79B9_7F4A_7C15 ^ (i as u64 + 1) });
    }
    plan
}

/// `name,input,program,output,format`, one show per feed, each sending its
/// copy (or `format`) to the i-th address after `out`.
pub fn csv(rows: &[Row], out: SocketAddrV4, format: &str) -> String {
    let mut s = String::from("name,input,program,output,format\n");
    for (i, r) in rows.iter().enumerate() {
        let _ = writeln!(s, "{},{},{},udp://{},{}", r.name, r.input, r.program, net::nth(out, i as u32, false), format);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feeds::clip::tests::synthetic;

    #[test]
    fn reads_impairments() {
        assert_eq!(impairment("0-9:loss=1,jitter=20").unwrap(), (0, 9, Impair { loss: 1.0, jitter_ms: 20.0 }));
        assert_eq!(impairment("17:loss=0.5").unwrap().1, 17);
        assert!(impairment("x:loss=1").is_err());
        assert!(impairment("3:wobble=1").is_err());
    }

    #[test]
    fn spreads_feeds_over_addresses_and_writes_the_list() {
        let clip = Arc::new(Clip::from_bytes("a.ts".into(), synthetic(700, 2700)).unwrap());
        let special = [(1, 1, Impair { loss: 2.0, jitter_ms: 0.0 })];
        let p = build(&[clip], 3, ("239.77.0.1:5000".parse().unwrap(), false), Impair::default(), &special);
        assert_eq!(p.feeds[2].to, "239.77.0.3:5000".parse().unwrap());
        assert_eq!((p.feeds[0].loss, p.feeds[1].loss), (0.0, 2.0));
        assert_ne!(p.feeds[0].start, p.feeds[1].start);
        let text = csv(&p.rows, "127.0.0.1:30000".parse().unwrap(), "copy");
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "name,input,program,output,format");
        assert_eq!(lines[2], "feed-002,udp://@239.77.0.2:5000,1,udp://127.0.0.1:30001,copy");
        let uni = build(&p.feeds.iter().map(|f| f.clip.clone()).take(1).collect::<Vec<_>>(), 2, ("127.0.0.1:20000".parse().unwrap(), false), Impair::default(), &[]);
        assert_eq!(uni.rows[1].input, "udp://@:20001");
    }
}
