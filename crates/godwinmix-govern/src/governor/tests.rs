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
    g.set_elsewhere(3000);
    assert_eq!(g.headroom(None).cpu_millicores, 8000 - 666 - 3100, "once, as the station's own");
}
