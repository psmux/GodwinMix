//! The sampler on this machine, for real: no fake counters.

use super::*;
use std::sync::atomic::AtomicBool;

/// Spin `threads` threads until `stop` is set.
fn burn(threads: usize, stop: Arc<AtomicBool>) -> Vec<std::thread::JoinHandle<()>> {
    (0..threads)
        .map(|_| {
            let stop = stop.clone();
            std::thread::spawn(move || {
                let mut x: u64 = 1;
                while !stop.load(Ordering::Relaxed) {
                    x = std::hint::black_box(x.wrapping_mul(6364136223846793005).wrapping_add(1));
                }
            })
        })
        .collect()
}

#[test]
fn this_machine_answers_every_question() {
    assert!(sys::system_ticks().is_some(), "system CPU counters");
    assert!(sys::process_cpu_ns().is_some(), "process CPU counters");
    let (total, avail) = sys::memory_mib().expect("memory figures");
    assert!(total > 256 && avail <= total, "{total} {avail}");
}

#[test]
fn two_busy_threads_show_as_about_two_cores_of_this_process() {
    let mut probe = Probe::new();
    std::thread::sleep(Duration::from_millis(50));
    let _ = probe.read();
    let stop = Arc::new(AtomicBool::new(false));
    let workers = burn(2, stop.clone());
    std::thread::sleep(Duration::from_millis(400));
    let r = probe.read();
    stop.store(true, Ordering::Relaxed);
    for w in workers {
        w.join().unwrap();
    }
    // Other tests run beside this one in the same process, so this is a
    // floor, not an equality.
    assert!(r.own_millicores >= 1500, "own {r:?}");
    assert!(r.system_millicores >= r.own_millicores, "system {r:?}");
}

#[test]
fn the_sampler_fills_the_cell_and_stops_when_dropped() {
    let cell = Arc::new(LoadCell::default());
    let s = Sampler::start(cell.clone(), Duration::from_millis(20)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while cell.load().samples < 3 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(s);
    let l = cell.load();
    assert!(l.samples >= 3, "{l:?}");
    assert!(l.available_mib.is_some());
    let after = cell.load().samples;
    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(cell.load().samples, after, "no sample after the drop");
}

/// The sampler's own cost. One read plus the smoothing, timed over many
/// rounds; at one read a second that is the fraction of a core it takes.
#[test]
fn a_sample_costs_well_under_a_thousandth_of_a_core_at_one_a_second() {
    let mut probe = Probe::new();
    let mut window = Window::new(10);
    let cell = LoadCell::default();
    let rounds = 2000;
    let t0 = Instant::now();
    for _ in 0..rounds {
        cell.store(&window.push(probe.read()));
    }
    let wall = t0.elapsed();
    let per = wall / rounds;
    // Wall time on one thread bounds its CPU time from above.
    let share = per.as_secs_f64();
    eprintln!("governor sample: {:?} each, {:.4}% of one core at 1 Hz", per, share * 100.0);
    // Wall time, so a shared runner that declares itself slow with
    // GODWINMIX_TIMING_SLACK gets the same multiple the other timing tests
    // do: a Windows runner measured 1.9 ms a sample where a desk takes well
    // under one.
    let slack = std::env::var("GODWINMIX_TIMING_SLACK").ok().and_then(|s| s.parse::<f64>().ok()).unwrap_or(1.0).max(1.0);
    assert!(share < 0.001 * slack, "{per:?} a sample is {:.4}% of a core", share * 100.0);
}
