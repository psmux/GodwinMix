use super::*;

fn load(own: u32, others: u32) -> Load {
    Load { own_millicores: own, others_peak_millicores: others, system_millicores: own + others, samples: 5, ..Default::default() }
}

fn inputs(l: &Load) -> Inputs<'_> {
    Inputs { cores: 8, memory_total_mib: 16_384, load: l, committed: Cost::default(), reserve_millicores: 1000, device: None, uplink_kbps: None }
}

#[test]
fn a_desktop_keeps_more_back_than_a_server() {
    assert_eq!(reserve(8, true, 0, None), 1600);
    assert_eq!(reserve(8, false, 0, None), 666);
    assert_eq!(reserve(4, true, 0, None), 1500, "at least a core and a half with the page on the machine");
    assert_eq!(reserve(4, false, 0, None), 500);
}

#[test]
fn a_bursty_machine_keeps_more_but_never_more_than_half() {
    assert_eq!(reserve(8, false, 400, None), 1066);
    assert_eq!(reserve(2, true, 5000, None), 1000);
}

#[test]
fn the_override_replaces_the_worked_out_reserve() {
    assert_eq!(reserve(8, true, 900, Some(3000)), 3000);
    assert_eq!(reserve(2, true, 0, Some(9000)), 2000);
}

#[test]
fn what_is_left_takes_off_other_programs_at_their_peak() {
    let l = load(500, 2000);
    let h = have(&inputs(&l));
    assert_eq!(h.cpu_millicores, 8000 - 1000 - 2000 - 500);
}

#[test]
fn a_promise_counts_before_it_shows_in_the_measurement() {
    let l = load(500, 0);
    let mut i = inputs(&l);
    i.committed = Cost { cpu_millicores: 3000, ..Cost::default() };
    assert_eq!(have(&i).cpu_millicores, 8000 - 1000 - 3000);
}

#[test]
fn a_device_limit_counts_sessions_already_held() {
    let l = load(0, 0);
    let mut i = inputs(&l);
    i.device = Some(DeviceUse { committed_millis: 300, committed_sessions: 2, session_limit: Some(3), measured_millis: None });
    let h = have(&i);
    assert_eq!(h.device_sessions, 1);
    assert_eq!(h.device_millis, 600);
    i.device = Some(DeviceUse { measured_millis: Some(800), ..DeviceUse::default() });
    let h = have(&i);
    assert_eq!(h.device_millis, 100, "the platform's own figure wins when it is higher");
    assert_eq!(h.device_sessions, UNLIMITED);
}

#[test]
fn memory_and_uplink_are_counted_when_known() {
    let mut l = load(0, 0);
    l.available_mib = Some(4096);
    let mut i = inputs(&l);
    i.uplink_kbps = Some(10_000);
    i.committed.egress_kbps = 6000;
    let h = have(&i);
    assert_eq!(h.memory_mib, 4096 - 1638);
    assert_eq!(h.egress_kbps, 2000);
    assert_eq!(short(&Cost { egress_kbps: 3000, ..Cost::default() }, &h), vec!["uplink"]);
}
