//! The seam to the station's governor.
//!
//! There is one governor for the whole machine (Decision 3 of
//! `dev/plans/shows-and-renditions.md`), and the station owns it. Wiring it
//! into the station is the graph work's; when that lands, the station hands
//! its governor over with [`Seam::use_station`] (through
//! `Channels::use_governor`) before the first channel asks for anything, and
//! from then on the channels' transcodes and the programme's renditions are
//! counted against one budget.
//!
//! Until then, and in a core that has no station governor, the channels make
//! one of their own the first time a destination asks for a rendition, from
//! the calibration on disk if there is one and from cautious figures if not,
//! and start its load sampler. Nothing is made for a channel that only
//! copies.

use std::path::PathBuf;
use std::sync::OnceLock;

use godwinmix_govern::store::Store;
use godwinmix_govern::{Governor, GovernorConfig, Profile};
use tracing::{info, warn};

pub struct Seam {
    station: OnceLock<Governor>,
    own: OnceLock<Governor>,
    /// Where calibrations are kept: the runtime store's directory.
    data_dir: Option<PathBuf>,
}

impl Seam {
    pub fn new(data_dir: Option<PathBuf>) -> Seam {
        Seam { station: OnceLock::new(), own: OnceLock::new(), data_dir }
    }

    /// Count against the station's governor from now on. Answers false when
    /// one was already set.
    pub fn use_station(&self, governor: Governor) -> bool {
        self.station.set(governor).is_ok()
    }

    /// The governor to ask: the station's, or the channels' own.
    pub fn get(&self) -> Governor {
        if let Some(g) = self.station.get() {
            return g.clone();
        }
        self.own.get_or_init(|| self.make()).clone()
    }

    fn make(&self) -> Governor {
        let calibration = self.data_dir.as_deref().and_then(|d| Store::new(d).latest());
        let profile = match calibration {
            Some(c) => {
                info!(fingerprint = %c.fingerprint, "channel transcodes are admitted against this machine's calibration");
                Profile::from_calibration(c)
            }
            None => Profile::uncalibrated(),
        };
        let governor = Governor::new(GovernorConfig::default(), profile);
        if let Err(e) = governor.start_sampling() {
            warn!(%e, "the governor could not sample this machine's load; it admits on its own book alone");
        }
        governor
    }
}
