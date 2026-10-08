//! Which outputs come back by themselves after a restart.
//!
//! A destination added from the UI is saved in the runtime store and started
//! again at boot, because a church that set YouTube up once expects it there
//! next Sunday. A recording is different. Somebody pressed Record for this
//! service, and a mixer that starts writing files again after a quit and a
//! reopen is one nobody can stop: the tester who reported it had no idea a
//! file was growing. So a recording started at runtime lives until it is
//! stopped or the mixer exits, and is never written to the runtime store.
//!
//! Recording whenever the mixer runs is still possible, and it is explicit:
//! write the recorder into the config file's `[[outputs]]`. Those are kept
//! even once the runtime store has its own output list, because the store
//! never holds a recording that could stand in for them.

use super::OutputConfig;

/// The provide ids that write a file rather than send a stream.
const RECORDERS: &[&str] = &["record/output", "file-record/output"];

impl OutputConfig {
    /// True for an output that writes the programme to a file on the mixer.
    pub fn is_recording(&self) -> bool {
        match self.type_id.as_deref() {
            Some(kind) => RECORDERS.contains(&kind),
            None => self.uri.starts_with("record://"),
        }
    }
}

/// What the runtime store saves: every output except the recordings.
pub fn kept_across_restart(outputs: &[OutputConfig]) -> Vec<OutputConfig> {
    outputs.iter().filter(|o| !o.is_recording()).cloned().collect()
}

/// The outputs to start with when the runtime store has a list.
///
/// The store's destinations, less any recording an older release saved there
/// (0.2.2 did, which is how a recording came back on its own), plus the
/// recordings the config file asks for.
pub(super) fn at_boot(file: &[OutputConfig], stored: Vec<OutputConfig>) -> Vec<OutputConfig> {
    let dropped: Vec<&str> =
        stored.iter().filter(|o| o.is_recording()).map(|o| o.id.as_str()).collect();
    if !dropped.is_empty() {
        tracing::info!(
            outputs = ?dropped,
            "not starting recordings saved by an earlier run; press Record to start a new one, \
             or write the recorder into the config file to record whenever the mixer runs"
        );
    }
    let mut outputs = kept_across_restart(&stored);
    for wanted in file.iter().filter(|o| o.is_recording()) {
        if !outputs.iter().any(|o| o.id == wanted.id) {
            outputs.push(wanted.clone());
        }
    }
    outputs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recorder(id: &str) -> OutputConfig {
        let mut out = OutputConfig::bare(id, "record://programme");
        out.type_id = Some("record/output".into());
        out
    }

    #[test]
    fn recordings_are_told_apart_from_destinations() {
        assert!(recorder("rec").is_recording());
        assert!(OutputConfig::bare("rec", "record://programme").is_recording());
        let mut sidecar = OutputConfig::bare("archive", "");
        sidecar.type_id = Some("file-record/output".into());
        assert!(sidecar.is_recording());
        assert!(!OutputConfig::bare("yt", "rtmp://a/live/key").is_recording());
    }

    /// The tester's runtime file: a destination and a recording pressed in
    /// the UI. The destination comes back and the recording does not.
    #[test]
    fn a_saved_recording_does_not_start_at_boot() {
        let stored = vec![OutputConfig::bare("yt", "rtmp://a/live/key"), recorder("recording-m1x2")];
        let outputs = at_boot(&[], stored);
        let ids: Vec<&str> = outputs.iter().map(|o| o.id.as_str()).collect();
        assert_eq!(ids, ["yt"]);
    }

    /// A recorder in the config file is the explicit way to record on start,
    /// and the runtime store having a list of its own must not lose it.
    #[test]
    fn a_recorder_in_the_config_file_still_starts() {
        let file = vec![OutputConfig::bare("old", "rtmp://b/live/key"), recorder("archive")];
        let stored = vec![OutputConfig::bare("yt", "rtmp://a/live/key")];
        let ids: Vec<String> = at_boot(&file, stored).into_iter().map(|o| o.id).collect();
        assert_eq!(ids, ["yt", "archive"]);
        assert!(kept_across_restart(&[recorder("archive")]).is_empty());
    }
}
