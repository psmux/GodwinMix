//! CPU time and memory: this process's own, and every process on the
//! machine with its parent, for following a station's tree.

use std::process::Command;

/// User plus system CPU seconds this process has used.
pub fn own_cpu_seconds() -> f64 {
    // SAFETY: getrusage fills the struct it is given.
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) };
    let tv = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
    tv(ru.ru_utime) + tv(ru.ru_stime)
}

/// The most memory this process has held, in MiB.
pub fn own_rss_mib() -> f64 {
    // SAFETY: as above.
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) };
    let bytes = if cfg!(target_os = "macos") { ru.ru_maxrss as f64 } else { ru.ru_maxrss as f64 * 1024.0 };
    (bytes / 1048576.0 * 10.0).round() / 10.0
}

#[derive(Clone, Debug, PartialEq)]
pub struct Proc {
    pub pid: u32,
    pub ppid: u32,
    pub cpu_seconds: f64,
    pub rss_kib: u64,
    pub command: String,
}

/// Every process now. `ps` on macOS (its `time` has hundredths), `/proc` on Linux.
pub fn all() -> Vec<Proc> {
    if cfg!(target_os = "linux") {
        return linux();
    }
    let out = Command::new("ps").args(["-A", "-o", "pid=,ppid=,rss=,time=,command="]).output();
    out.map(|o| String::from_utf8_lossy(&o.stdout).lines().filter_map(ps_line).collect()).unwrap_or_default()
}

fn ps_line(line: &str) -> Option<Proc> {
    let mut it = line.split_whitespace();
    let (pid, ppid, rss, time) = (it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?.parse().ok()?, it.next()?);
    let command = it.collect::<Vec<_>>().join(" ");
    Some(Proc { pid, ppid, rss_kib: rss, cpu_seconds: clock(time)?, command })
}

/// `1:02.53`, `12:01:02.53` or `2-01:00:00` as seconds.
fn clock(t: &str) -> Option<f64> {
    let (days, rest) = match t.split_once('-') {
        Some((d, r)) => (d.parse::<f64>().ok()?, r),
        None => (0.0, t),
    };
    let secs = rest.split(':').try_fold(0.0, |acc, part| part.parse::<f64>().ok().map(|v| acc * 60.0 + v))?;
    Some(days * 86400.0 + secs)
}

fn linux() -> Vec<Proc> {
    let tick = 100.0;
    let page_kib = 4;
    let Ok(dir) = std::fs::read_dir("/proc") else { return Vec::new() };
    dir.filter_map(|e| e.ok()?.file_name().to_str()?.parse::<u32>().ok())
        .filter_map(|pid| {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
            let after = &stat[stat.rfind(')')? + 2..];
            let f: Vec<&str> = after.split_whitespace().collect();
            let cmd = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
            let command = String::from_utf8_lossy(&cmd).replace('\0', " ").trim().to_string();
            let cpu = (f.get(11)?.parse::<f64>().ok()? + f.get(12)?.parse::<f64>().ok()?) / tick;
            Some(Proc { pid, ppid: f.get(1)?.parse().ok()?, cpu_seconds: cpu, rss_kib: f.get(21)?.parse::<u64>().ok()? * page_kib, command })
        })
        .collect()
}

/// `root` and everything started under it, root first.
pub fn tree(all: &[Proc], root: u32) -> Vec<Proc> {
    let mut out: Vec<Proc> = all.iter().filter(|p| p.pid == root).cloned().collect();
    let mut i = 0;
    while i < out.len() {
        let parent = out[i].pid;
        out.extend(all.iter().filter(|p| p.ppid == parent && p.pid != parent).cloned());
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_ps_lines_and_clocks() {
        let p = ps_line("  412   1  16160 1:02.53 /usr/bin/godwinmix --show cam --bind 127.0.0.1:0").unwrap();
        assert_eq!((p.pid, p.ppid, p.rss_kib), (412, 1, 16160));
        assert!((p.cpu_seconds - 62.53).abs() < 1e-9);
        assert!(p.command.ends_with("127.0.0.1:0"));
        assert_eq!(clock("2-01:00:00"), Some(176400.0));
        assert_eq!(clock("12:01:02.5"), Some(43262.5));
    }

    #[test]
    fn follows_a_tree_and_finds_itself() {
        let p = |pid, ppid| Proc { pid, ppid, cpu_seconds: 0.0, rss_kib: 0, command: String::new() };
        let all = vec![p(1, 0), p(10, 1), p(11, 10), p(12, 10), p(13, 11), p(20, 1)];
        let pids: Vec<u32> = tree(&all, 10).iter().map(|p| p.pid).collect();
        assert_eq!(pids, vec![10, 11, 12, 13]);
        let me = std::process::id();
        assert!(all_now_has(me), "this test process is in the list");
        assert!(own_cpu_seconds() >= 0.0 && own_rss_mib() > 0.0);
    }

    fn all_now_has(pid: u32) -> bool {
        all().iter().any(|p| p.pid == pid && p.rss_kib > 0)
    }
}
