//! What the link tells the station, and what the station tells every client.

use super::link::serve::Host;
use super::link::Hello;
use super::state::Station;
use godwinmix_govern::Governor;
use godwinmix_protocol::shows::{Show, ShowState};
use godwinmix_protocol::types::Event;
use std::sync::Arc;
use tracing::{info, warn};

/// The link holds the station through this, so a test can stand in for it.
pub struct Linked(pub Arc<Station>);

impl Host for Linked {
    fn hello(&self, hello: &Hello) -> bool {
        let st = &self.0;
        {
            let mut procs = st.procs.lock();
            let Some(p) = procs.get_mut(&hello.show) else {
                warn!(show = %hello.show, "a process said hello for a show this station does not have");
                return false;
            };
            if p.secret.is_empty() || p.secret != hello.secret {
                warn!(show = %hello.show, "a hello with the wrong secret; turned away");
                return false;
            }
            p.state = ShowState::Running;
            p.error = None;
            p.addr.send_replace(Some(hello.addr));
        }
        info!(show = %hello.show, addr = %hello.addr, "show is running");
        st.announce(&hello.show);
        let (fed, id) = (st.clone(), hello.show.clone());
        tokio::spawn(async move { super::direct::feed_source(&fed, &id).await });
        true
    }

    fn on_air(&self, show: &str, on: bool) {
        let st = &self.0;
        let any = {
            let mut set = st.on_air.lock();
            match on {
                true => set.insert(show.to_string()),
                false => set.remove(show),
            };
            !set.is_empty()
        };
        st.render.set_on_air(any);
    }

    fn load(&self, show: &str, millicores: u32) {
        let st = &self.0;
        let sum = {
            let mut loads = st.loads.lock();
            loads.insert(show.to_string(), millicores);
            loads.values().sum()
        };
        st.render.governor().set_elsewhere(sum);
    }

    fn gone(&self, show: &str, pid: u32) {
        let st = &self.0;
        let sum = {
            let mut loads = st.loads.lock();
            loads.remove(show);
            loads.values().sum()
        };
        st.render.governor().set_elsewhere(sum);
        let mut procs = st.procs.lock();
        if let Some(p) = procs.get_mut(show).filter(|p| p.pid == Some(pid)) {
            p.addr.send_replace(None);
        }
        drop(procs);
        let any = {
            let mut set = st.on_air.lock();
            set.remove(show);
            !set.is_empty()
        };
        st.render.set_on_air(any);
    }

    fn governor(&self) -> Governor {
        self.0.render.governor().clone()
    }
}

impl Station {
    /// One show as a client sees it, from what the station knows without
    /// asking the show. `show.list` fills in the rest.
    pub fn view(&self, id: &str) -> Option<Show> {
        let record = self.registry.lock().get(id)?.clone();
        let (state, restarts, error) = {
            let procs = self.procs.lock();
            let p = procs.get(id)?;
            (p.state, p.restarts, p.error.clone())
        };
        let mut show = Show {
            id: id.to_string(),
            name: record.name.clone(),
            state,
            on_air: None,
            programme_kbps: 0,
            cpu_millicores: 0,
            memory_mib: 0,
            restarts,
            error,
            compositing: record.compositing,
            input: record.input.clone(),
            outputs: Vec::new(),
            health: self.direct.health_of(self, id),
            alarms: record.alarms.clone(),
        };
        if !record.compositing {
            show.state = if record.stopped { ShowState::Stopped } else { ShowState::Running };
            show.outputs = self.direct.output_views(self, id);
            show.programme_kbps = show.outputs.iter().map(|o| u64::from(o.live.kbps)).sum();
            show.on_air = self.direct.seen.lock().get(id).filter(|s| s.input_live()).map(|_| "input".to_string());
        }
        Some(show)
    }

    /// `event/show.changed` for one show, to every client.
    pub fn announce(&self, id: &str) {
        if let Some(show) = self.view(id) {
            self.events.emit(Event::ShowChanged { show: Box::new(show) });
        }
    }
}
