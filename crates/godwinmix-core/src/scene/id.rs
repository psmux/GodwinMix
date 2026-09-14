//! Identifiers for scene document nodes.
//!
//! Source and output ids stay slugs: an operator types them and reads them in
//! errors. Scene document nodes are different. Nobody types the id of an item,
//! every override addresses one, and two clients may mint ids at the same time
//! with no coordination, so they are UUIDs (11 section 2). Version 7 gives a
//! time ordered id, which means a document's records sort into creation order
//! for free and a diff of two saves stays readable.
//!
//! The `uuid` crate is not a dependency. All that is wanted here is generating
//! a v7, deriving a v8, and parsing and printing the hyphenated form, which is
//! under a hundred lines; `getrandom` is already in the dependency tree under
//! rustls and supplies the sixteen bytes.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A UUID, held as its 128 bits and printed in the hyphenated form.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Id(u128);

/// Highest (timestamp, sequence) handed out so far, so two ids minted in the
/// same millisecond still sort in the order they were created.
static LAST: AtomicU64 = AtomicU64::new(0);

impl Id {
    /// A fresh UUIDv7: 48 bits of Unix milliseconds, the version nibble, 12
    /// bits of counter, the variant bits, 62 bits of randomness.
    pub fn new() -> Id {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
            & 0xffff_ffff_ffff;
        let stamped = Self::next_stamp(ms);
        let (ms, seq) = (stamped >> 12, stamped & 0xfff);
        let mut rand = [0u8; 8];
        // A failure here means the OS has no entropy source at all. Falling
        // back to the clock keeps the id unique enough to carry on rather than
        // taking a live mixer down over an item id.
        if getrandom::fill(&mut rand).is_err() {
            rand = ms.rotate_left(17).to_be_bytes();
        }
        let low = u64::from_be_bytes(rand);
        let hi = ((ms as u128) << 16) | (0x7 << 12) | seq as u128;
        Id((hi << 64) | Self::variant(low))
    }

    /// Bump the shared clock to at least `ms`, returning `ms << 12 | seq`.
    fn next_stamp(ms: u64) -> u64 {
        let mut prev = LAST.load(Ordering::Relaxed);
        loop {
            let next = if ms > (prev >> 12) {
                ms << 12
            } else {
                prev + 1
            };
            match LAST.compare_exchange_weak(prev, next, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => return next,
                Err(seen) => prev = seen,
            }
        }
    }

    /// An id derived from a namespace and a name, the same every time.
    ///
    /// Applying a layout twice to the same scene has to land on the same items,
    /// or "grow the inset to full screen" becomes a cut instead of a ramp (11
    /// section 6a). A derived id cannot be a v7, because there is no clock in
    /// it, so it is a v8: RFC 9562 reserves that version for exactly this, an
    /// id whose bits the application chose. The hash is FNV-1a over 128 bits,
    /// picked because its definition cannot move under a persisted id the way
    /// `DefaultHasher` explicitly may.
    pub fn derive(namespace: &Id, name: &str) -> Id {
        const OFFSET: u128 = 0x6c62272e_07bb0142_62b82175_6295c58d;
        const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013B;
        let mut h = OFFSET;
        for b in namespace.0.to_be_bytes().iter().chain(name.as_bytes()) {
            h ^= *b as u128;
            h = h.wrapping_mul(PRIME);
        }
        let hi = (h >> 64) as u64;
        let hi = (hi & !0xf000) | 0x8000; // version 8
        Id(((hi as u128) << 64) | Self::variant(h as u64))
    }

    /// Set the two RFC 9562 variant bits on the low half.
    fn variant(low: u64) -> u128 {
        ((low & !(0b11 << 62)) | (0b10 << 62)) as u128
    }

    /// Parse the hyphenated form. Anything else is refused, because an id that
    /// is nearly a UUID is the bug that took OBS years to unpick.
    pub fn parse(s: &str) -> Result<Id, IdError> {
        let b = s.as_bytes();
        if b.len() != 36 || [8, 13, 18, 23].iter().any(|&i| b[i] != b'-') {
            return Err(IdError(s.to_string()));
        }
        let mut n: u128 = 0;
        for (i, c) in b.iter().enumerate() {
            if matches!(i, 8 | 13 | 18 | 23) {
                continue;
            }
            let d = (*c as char)
                .to_digit(16)
                .ok_or_else(|| IdError(s.to_string()))?;
            n = (n << 4) | d as u128;
        }
        Ok(Id(n))
    }

    /// The version nibble, 7 for a minted id and 8 for a derived one.
    pub fn version(&self) -> u8 {
        ((self.0 >> 76) & 0xf) as u8
    }
}

/// The id was not a hyphenated UUID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdError(pub String);

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} is not a UUID. Ids in a scene document are hyphenated UUIDs, for example 0192f3a4-1b2c-7d3e-8f40-51a2b3c4d5e6", self.0)
    }
}

impl std::error::Error for IdError {}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let h = format!("{:032x}", self.0);
        write!(
            f,
            "{}-{}-{}-{}-{}",
            &h[0..8],
            &h[8..12],
            &h[12..16],
            &h[16..20],
            &h[20..32]
        )
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

impl Serialize for Id {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Id, D::Error> {
        let s = String::deserialize(d)?;
        Id::parse(&s).map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for Id {
    fn schema_name() -> Cow<'static, str> {
        "Id".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "description": "A UUID in the hyphenated form. Minted ids are version 7 (time ordered); ids derived from a layout are version 8.",
            "pattern": "^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$"
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minted_ids_are_v7_and_round_trip_through_text() {
        let id = Id::new();
        assert_eq!(id.version(), 7);
        assert_eq!(Id::parse(&id.to_string()).unwrap(), id);
        assert_eq!(id.to_string().len(), 36);
    }

    #[test]
    fn minted_ids_sort_into_creation_order_even_within_one_millisecond() {
        let ids: Vec<Id> = (0..500).map(|_| Id::new()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "v7 ids minted in a burst must stay ordered");
        let unique: std::collections::BTreeSet<_> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
    }

    #[test]
    fn derived_ids_are_v8_stable_and_namespaced() {
        let scene = Id::new();
        let other = Id::new();
        let a = Id::derive(&scene, "inset");
        assert_eq!(a, Id::derive(&scene, "inset"));
        assert_eq!(a.version(), 8);
        assert_ne!(a, Id::derive(&scene, "main"));
        assert_ne!(a, Id::derive(&other, "inset"));
        assert_eq!(Id::parse(&a.to_string()).unwrap(), a);
    }

    #[test]
    fn a_near_miss_is_refused_with_an_example_in_the_message() {
        for bad in [
            "",
            "cam1",
            "0192f3a41b2c7d3e8f4051a2b3c4d5e6",
            "0192f3a4-1b2c-7d3e-8f40-51a2b3c4d5eZ",
        ] {
            let e = Id::parse(bad).unwrap_err();
            assert!(e.to_string().contains("hyphenated UUID"), "{e}");
        }
    }
}
