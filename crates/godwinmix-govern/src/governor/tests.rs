use super::*;
use crate::testing::{gpu, profile, shape, x264};
use crate::{Admit, Kind, ShedAction};

/// Eight cores, headless, so the reserve is 666 with a still machine.
fn governor() -> Governor {
    Governor::with_machine(GovernorConfig::default(), profile(), 8, 16_384)
}

fn cpu(m: u32) -> Cost {
    Cost { cpu_millicores: m, ..Cost::default() }
}

fn still(g: &Governor, own: u32, others: u32) {
    g.load_cell().store(&Load {
        system_millicores: own + others,
        own_millicores: own,
        others_peak_millicores: others,
        samples: 3,
        ..Default::default()
    });
}

#[test]
fn work_that_fits_is_granted_and_held() {
    let g = governor();
    let t = g.admit(cpu(3000), "the programme").granted().expect("fits");
    assert_eq!(t.cost(), cpu(3000));
    assert_eq!(g.held().len(), 1);
    assert_eq!(g.headroom(None).cpu_millicores, 8000 - 666 - 3000);
}

#[test]
fn dropping_a_ticket_gives_its_share_back() {
    let g = governor();
    let a = g.admit(cpu(4000), "a").granted().unwrap();
    assert!(g.admit(cpu(4000), "b").granted().is_none(), "no room for a second");
    drop(a);
    assert!(g.held().is_empty());
    assert!(g.admit(cpu(4000), "b").granted().is_some(), "room again");
}

#[test]
fn a_refusal_says_what_it_needs_what_is_free_and_what_fits() {
    let g = governor();
    still(&g, 0, 4000);
    let Admit::Refused { need, have, advice } = g.admit(cpu(6000), "the 1080p60 HEVC rendition") else { panic!("should not fit") };
    assert_eq!(need.cpu_millicores, 6000);
    assert_eq!(have.cpu_millicores, 8000 - 666 - 4000);
    assert_eq!(advice.short, ["cpu"]);
    assert!(advice.text.starts_with("the 1080p60 HEVC rendition needs 6.0 cores and 3.3 cores is free."), "{}", advice.text);
    assert!(advice.text.contains("fits, or"), "a software and a GPU choice: {}", advice.text);
    assert!(advice.fits.iter().any(|f| f.slot == x264() && f.height == 1080 && f.fps.num == 30), "{:?}", advice.fits);
    assert!(advice.fits.iter().any(|f| f.slot.hardware));
    let data = g.admit(cpu(6000), "x").error_data().unwrap();
    assert!(data["fits"].is_array() && data["need"]["cpu_millicores"] == 6000);
}

#[test]
fn a_device_refuses_past_the_session_limit_it_showed() {
    let g = governor();
    let s = shape(1280, 720, 30);
    let held: Vec<_> = (0..3).map(|i| g.admit_encode(&gpu(), &s, &format!("rung {i}"), Kind::Other).granted().expect("fits")).collect();
    let Admit::Refused { advice, .. } = g.admit_encode(&gpu(), &s, "a fourth", Kind::Other) else { panic!("the device refused a fourth") };
    assert!(advice.short.contains(&"sessions"), "{advice:?}");
    assert!(advice.fits.iter().all(|f| !f.slot.hardware), "no GPU offered while it is full");
    drop(held);
    assert!(g.admit_encode(&gpu(), &s, "a fourth", Kind::Other).granted().is_some());
}

#[test]
fn a_software_encode_that_does_not_fit_is_offered_a_faster_preset() {
    let g = governor();
    still(&g, 0, 5500);
    // 1834 free: veryfast at 1080p30 is 2000, superfast 1400.
    let t = g.admit_encode(&x264(), &shape(1920, 1080, 30), "programme", Kind::Programme).granted().expect("fits at a faster preset");
    assert_eq!(t.preset(), Some("superfast"));
    assert!(t.cost().cpu_millicores <= 1400);
}

#[test]
fn shedding_starts_only_once_half_the_reserve_is_eaten() {
    let g = governor();
    let _p = g.admit_encode(&x264(), &shape(1920, 1080, 30), "programme", Kind::Programme).granted().unwrap();
    let _m = g.admit_claim(Claim::new("multiview", cpu(400)).kind(Kind::Preview)).granted().unwrap();
    still(&g, 2400, 5000);
    assert!(g.shed().is_empty(), "7400 is inside the line plus half the reserve");
    still(&g, 2400, 5400);
    let steps = g.shed();
    assert_eq!(steps.first().map(|s| s.what.as_str()), Some("multiview"));
    assert_eq!(steps[0].action, ShedAction::Drop);
}

#[test]
fn a_lowered_ticket_holds_its_new_cost() {
    let g = governor();
    let mut t = g.admit_encode(&x264(), &shape(1920, 1080, 30), "p", Kind::Programme).granted().unwrap();
    t.lower("ultrafast", cpu(1000));
    assert_eq!(g.held()[0].cost, cpu(1000));
    assert_eq!(g.headroom(None).cpu_millicores, 8000 - 666 - 1000);
}

#[test]
fn many_threads_admitting_and_dropping_leave_the_book_empty() {
    let g = governor();
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let g = g.clone();
            std::thread::spawn(move || {
                for _ in 0..500 {
                    let t = g.admit(cpu(700), "x").granted();
                    drop(t);
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    assert!(g.held().is_empty());
}

#[test]
fn admission_and_release_take_microseconds() {
    let g = governor();
    let _programme = g.admit(cpu(2000), "programme").granted().unwrap();
    let rounds = 10_000u32;
    let t0 = std::time::Instant::now();
    for _ in 0..rounds {
        drop(g.admit(cpu(500), "x").granted());
    }
    let per = t0.elapsed() / rounds;
    eprintln!("governor admit and release: {per:?} each");
    assert!(per < std::time::Duration::from_micros(50), "{per:?}");
}

#[test]
fn a_ticket_outliving_its_governor_is_harmless() {
    let g = governor();
    let t = g.admit(cpu(10), "x").granted().unwrap();
    drop(g);
    drop(t);
}

#[test]
fn a_shows_measured_work_is_not_counted_twice_by_its_station() {
    let g = governor();
    let _t = g.admit(cpu(3000), "show a's programme").granted().unwrap();
    // The station's sampler sees show a's encoder as another program.
    still(&g, 100, 3000);
    assert_eq!(g.headroom(None).cpu_millicores, 8000 - 666 - 3000 - 3000, "counted twice");
    // The show says what it measures; the sampler's next reading has it.
    g.set_elsewhere(3000);
    let r = crate::load::Reading { system_millicores: 3100, own_millicores: 100, ..Default::default() };
    g.load_cell().store(&crate::load::Window::new(10).push(r.with_elsewhere(g.load_cell().elsewhere())));
    assert_eq!(g.headroom(None).cpu_millicores, 8000 - 666 - 3100, "once, as the station's own");
}

/// Sixteen cores with the page on them, the load as a sampler would leave
/// it after ten readings of `(system, own)`.
fn desktop_with(readings: &[(u32, u32)]) -> Governor {
    let config = GovernorConfig { desktop: true, ..GovernorConfig::default() };
    let g = Governor::with_machine(config, profile(), 16, 32_768);
    let mut w = crate::load::Window::new(10);
    for &(system, own) in readings {
        let r = crate::load::Reading { system_millicores: system, own_millicores: own, ..Default::default() };
        g.load_cell().store(&w.push(r));
    }
    g
}

#[test]
fn a_three_rung_ladder_on_a_busy_desktop_whose_browser_spikes_is_granted() {
    // 40 to 50% of sixteen cores, two spikes to 70% in the window, this
    // process steady at half a core. Counting the spike again as jitter made
    // the reserve 3.2 + 3.7 cores, which with other programs at 11 cores and
    // this process came to more than the machine: free read 0, and the top
    // rung's GPU encode, priced at a few hundredths of a core, was refused
    // with "needs 0.0 cores and 0.0 cores is free".
    let g = desktop_with(&[(7000, 500), (6500, 500), (11_500, 500), (7000, 500), (6400, 500), (8000, 500), (7200, 500), (11_000, 500), (6600, 500), (7000, 500)]);
    assert_eq!(g.reserve(), 3200, "the spike is in other programs' peak, not in the reserve as well");
    assert_eq!(g.headroom(None).cpu_millicores, 16_000 - 3200 - 11_000 - 500);
    let mut held = Vec::new();
    for (w, h) in [(1280, 720), (854, 480), (640, 360)] {
        let what = format!("the {h}p30 H.264 rendition for hls");
        let t = g.admit_encode(&gpu(), &shape(w, h, 30), &what, Kind::Rung { index: 0 }).granted();
        held.push(t.unwrap_or_else(|| panic!("{what} refused")));
        let scale = g.admit(cpu(180), &format!("{w}x{h} scaling for hls")).granted();
        held.push(scale.unwrap_or_else(|| panic!("the scale for {h}p refused")));
    }
    assert_eq!(g.held().len(), 6);
}

#[test]
fn a_refusal_of_a_few_hundredths_of_a_core_says_so() {
    // Free really is nothing here: other programs take the machine.
    let g = governor();
    still(&g, 0, 8000);
    let Admit::Refused { have, advice, .. } = g.admit(cpu(28), "the 360p30 H.264 rendition") else { panic!("nothing is free") };
    assert_eq!(have.cpu_millicores, 0);
    assert!(advice.text.starts_with("the 360p30 H.264 rendition needs 0.03 cores and 0 cores is free."), "{}", advice.text);
}
