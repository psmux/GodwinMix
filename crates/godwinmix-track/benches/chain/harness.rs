//! A small timing harness in the shape of criterion's `bench_function` and
//! `iter`, on the standard library alone. Criterion pulls in a C file (its
//! `alloca` dependency), which needs a C compiler for the target; this does
//! not, so the bench builds for `aarch64-unknown-linux-musl` from anywhere.
//!
//! Each function is warmed up for half a second, then timed in 30 samples of
//! a batch sized to take about 50 ms. It prints the median time an iteration
//! and the spread between the fastest and slowest sample. A first argument
//! that is not a flag filters the names, as `cargo bench -- motion` does.

use std::time::{Duration, Instant};

pub struct Criterion {
    filter: Option<String>,
}

impl Default for Criterion {
    fn default() -> Criterion {
        Criterion { filter: std::env::args().skip(1).find(|a| !a.starts_with('-')) }
    }
}

pub struct Bencher {
    samples: Vec<f64>,
}

impl Criterion {
    pub fn bench_function(&mut self, name: &str, mut f: impl FnMut(&mut Bencher)) -> &mut Criterion {
        if self.filter.as_deref().is_some_and(|w| !name.contains(w)) {
            return self;
        }
        let mut b = Bencher { samples: Vec::new() };
        f(&mut b);
        b.samples.sort_by(|x, y| x.total_cmp(y));
        let n = b.samples.len().max(1);
        let (lo, mid, hi) = (b.samples.first().copied().unwrap_or(0.0), b.samples.get(n / 2).copied().unwrap_or(0.0), b.samples.last().copied().unwrap_or(0.0));
        println!("{name:<56} time: [{} {} {}]", show(lo), show(mid), show(hi));
        self
    }
}

impl Bencher {
    pub fn iter<T>(&mut self, mut f: impl FnMut() -> T) {
        let warm = Instant::now();
        let mut runs = 0u64;
        while warm.elapsed() < Duration::from_millis(500) {
            std::hint::black_box(f());
            runs += 1;
        }
        let each = warm.elapsed().as_secs_f64() / runs.max(1) as f64;
        let batch = ((0.05 / each.max(1e-9)) as u64).clamp(1, 10_000_000);
        for _ in 0..30 {
            let t = Instant::now();
            for _ in 0..batch {
                std::hint::black_box(f());
            }
            self.samples.push(t.elapsed().as_secs_f64() / batch as f64);
        }
    }
}

fn show(seconds: f64) -> String {
    match seconds {
        s if s >= 1e-3 => format!("{:.3} ms", s * 1e3),
        s if s >= 1e-6 => format!("{:.3} µs", s * 1e6),
        s => format!("{:.1} ns", s * 1e9),
    }
}
