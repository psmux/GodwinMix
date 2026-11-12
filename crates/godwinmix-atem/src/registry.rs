//! Several switchers at once, each under a slug such as `atem-main`.

use std::collections::BTreeMap;

use crate::client::{Atem, Snapshot};
use crate::session::Timing;
use crate::Error;

#[derive(Default)]
pub struct Switchers {
    by_id: BTreeMap<String, Atem>,
    timing: Timing,
}

impl Switchers {
    /// Switchers whose sessions use other waits, for tests on loopback.
    pub fn with_timing(timing: Timing) -> Switchers {
        Switchers { by_id: BTreeMap::new(), timing }
    }

    /// Connect a switcher on the current tokio runtime.
    pub fn add(&mut self, id: &str, address: &str) -> Result<&Atem, Error> {
        self.add_on(&tokio::runtime::Handle::current(), id, address)
    }

    /// Connect a switcher on a given runtime.
    pub fn add_on(
        &mut self,
        runtime: &tokio::runtime::Handle,
        id: &str,
        address: &str,
    ) -> Result<&Atem, Error> {
        if !is_slug(id) {
            return Err(Error::BadSlug { slug: id.to_string() });
        }
        if self.by_id.contains_key(id) {
            return Err(Error::Exists { slug: id.to_string() });
        }
        if address.trim().is_empty() {
            let why = "it is empty".to_string();
            return Err(Error::BadAddress { address: address.to_string(), why });
        }
        let atem = Atem::connect_with(runtime, id, address.trim(), self.timing);
        Ok(self.by_id.entry(id.to_string()).or_insert(atem))
    }

    /// Disconnect a switcher. Its session closes when the handle drops.
    pub fn remove(&mut self, id: &str) -> Result<(), Error> {
        self.by_id.remove(id).map(drop).ok_or_else(|| self.unknown(id))
    }

    pub fn get(&self, id: &str) -> Result<&Atem, Error> {
        self.by_id.get(id).ok_or_else(|| self.unknown(id))
    }

    /// The named switcher, or the only one when no name is given.
    pub fn pick(&self, id: Option<&str>) -> Result<&Atem, Error> {
        match id {
            Some(id) => self.get(id),
            None => match self.by_id.len() {
                0 => Err(Error::NoSwitchers),
                1 => Ok(self.by_id.values().next().expect("one entry")),
                _ => Err(Error::WhichOne { known: self.ids() }),
            },
        }
    }

    pub fn ids(&self) -> Vec<String> {
        self.by_id.keys().cloned().collect()
    }

    pub fn list(&self) -> Vec<Snapshot> {
        self.by_id.values().map(Atem::snapshot).collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Atem> {
        self.by_id.values()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    fn unknown(&self, id: &str) -> Error {
        Error::Unknown { slug: id.to_string(), known: self.ids() }
    }
}

/// Lower case letters, digits and single hyphens, starting with a letter,
/// not ending in a hyphen, at most 64 characters.
pub fn is_slug(id: &str) -> bool {
    let bytes = id.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 64
        && bytes[0].is_ascii_lowercase()
        && !id.ends_with('-')
        && !id.contains("--")
        && bytes.iter().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs() {
        assert!(is_slug("atem-main"));
        assert!(is_slug("atem2"));
        for bad in ["", "Atem", "2atem", "atem-", "atem--b", "atem main", "atém"] {
            assert!(!is_slug(bad), "{bad}");
        }
    }

    #[tokio::test]
    async fn one_switcher_needs_no_name_and_two_do() {
        let mut switchers = Switchers::default();
        assert_eq!(switchers.pick(None).err(), Some(Error::NoSwitchers));
        switchers.add("atem-main", "127.0.0.1:1").unwrap();
        assert_eq!(switchers.pick(None).unwrap().id(), "atem-main");
        switchers.add("atem-overflow", "127.0.0.1:2").unwrap();
        assert!(matches!(switchers.pick(None), Err(Error::WhichOne { .. })));
        assert!(matches!(switchers.add("atem-main", "127.0.0.1:3"), Err(Error::Exists { .. })));
        assert!(matches!(switchers.get("nope"), Err(Error::Unknown { .. })));
        switchers.remove("atem-main").unwrap();
        assert_eq!(switchers.ids(), ["atem-overflow"]);
    }
}
