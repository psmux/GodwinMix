use super::*;
use crate::testing::{profile, shape, x264};
use crate::{GovernorConfig, Profile};
use godwinmix_protocol::rendition::Cost;
use parking_lot::Mutex;
use std::collections::BTreeMap;

/// A station in the same process, reached through JSON as the real link
/// reaches it, so what crosses is what a socket would carry.
struct Station {
    governor: Governor,
    tickets: Mutex<BTreeMap<u64, Ticket>>,
    up: std::sync::atomic::AtomicBool,
}

impl Remote for Station {
    fn ask(&self, ask: &Ask) -> Option<Answer> {
        if !self.up.load(std::sync::atomic::Ordering::SeqCst) {
            return None;
        }
        let wire: Ask = serde_json::from_str(&serde_json::to_string(ask).unwrap()).unwrap();
        let (answer, ticket) = self.governor.answer(wire);
        if let Some(t) = ticket {
            self.tickets.lock().insert(t.id(), t);
        }
        Some(serde_json::from_str(&serde_json::to_string(&answer).unwrap()).unwrap())
    }

    fn release(&self, ticket: u64) {
        self.tickets.lock().remove(&ticket);
    }
}

fn station() -> Arc<Station> {
    let governor = Governor::with_machine(GovernorConfig::default(), profile(), 8, 16_384);
    Arc::new(Station { governor, tickets: Mutex::new(BTreeMap::new()), up: true.into() })
}

fn show(station: &Arc<Station>) -> Governor {
    let g = Governor::with_machine(GovernorConfig::default(), Profile::uncalibrated(), 8, 16_384);
    g.set_remote(station.clone());
    g
}

fn cpu(m: u32) -> Cost {
    Cost { cpu_millicores: m, ..Cost::default() }
}

#[test]
fn two_shows_share_one_book_and_the_second_is_refused_what_the_first_holds() {
    let st = station();
    let (a, b) = (show(&st), show(&st));
    let held = a.admit(cpu(5000), "show a's programme").granted().expect("fits");
    assert_eq!(st.governor.held().len(), 1, "the station's book has it");
    assert!(a.held().is_empty(), "the show's own book has nothing");
    let refused = b.admit(cpu(5000), "show b's programme");
    let Admit::Refused { advice, .. } = refused else { panic!("two shows cannot both have five cores of eight") };
    assert_eq!(advice.short, vec!["cpu"]);
    drop(held);
    assert!(st.governor.held().is_empty(), "dropping the show's ticket gives the station's share back");
    assert!(b.admit(cpu(5000), "show b's programme").granted().is_some());
}

#[test]
fn an_encode_is_priced_by_the_station_not_by_a_show_that_never_measured() {
    let st = station();
    let a = show(&st);
    let t = a.admit_encode(&x264(), &shape(1920, 1080, 30), "1080p30", Kind::Programme).granted().unwrap();
    let priced = st.governor.profile().encode_cost(&x264(), &shape(1920, 1080, 30));
    assert_eq!(t.cost(), priced);
    assert_eq!(st.governor.held()[0].kind, Kind::Programme);
}

#[test]
fn with_the_station_gone_a_show_admits_against_its_own_book() {
    let st = station();
    let a = show(&st);
    st.up.store(false, std::sync::atomic::Ordering::SeqCst);
    let t = a.admit(cpu(1000), "while the station is away").granted().unwrap();
    assert_eq!(a.held().len(), 1);
    assert!(st.governor.held().is_empty());
    drop(t);
    assert!(a.held().is_empty(), "a local ticket goes back to the local book");
}

#[test]
fn a_station_that_drops_a_dead_shows_tickets_frees_the_machine() {
    let st = station();
    let a = show(&st);
    let t = a.admit(cpu(6000), "show a").granted().unwrap();
    // The show died: its process is gone and its tickets with it, so the
    // station throws away what it held for it. `release` from the leaked
    // ticket afterwards finds nothing, which is harmless.
    st.tickets.lock().clear();
    assert!(st.governor.held().is_empty());
    std::mem::forget(t);
}
