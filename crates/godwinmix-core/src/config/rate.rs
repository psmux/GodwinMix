//! The programme's frame rate, as a fraction so 29.97 is exact.
//!
//! Written the way people say it: `fps = 30`, `fps = 29.97`, `fps = "59.94"`
//! or `fps = "30000/1001"`. The broadcast rates 23.976, 29.97, 59.94 and
//! 119.88 are the NTSC fractions with 1001 under them; any other rate is a
//! whole number or a fraction given as one. A whole rate is written back as a
//! plain integer, so a config file from before reads and writes as it did.

use anyhow::{bail, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameRate {
    num: i32,
    den: i32,
}

/// The NTSC rates by what people call them.
const NTSC: [(&str, i32); 4] = [("23.976", 24000), ("29.97", 30000), ("59.94", 60000), ("119.88", 120000)];

impl FrameRate {
    pub const fn whole(n: i32) -> FrameRate {
        FrameRate { num: n, den: 1 }
    }

    pub fn numer(&self) -> i32 {
        self.num
    }

    pub fn denom(&self) -> i32 {
        self.den
    }

    pub fn as_f64(&self) -> f64 {
        self.num as f64 / self.den.max(1) as f64
    }

    /// The nearest whole number of frames a second, for whatever counts in
    /// frames: a keyframe interval, a frame budget, a page's paint rate.
    pub fn rounded(&self) -> i32 {
        self.as_f64().round() as i32
    }

    /// One frame's length in nanoseconds.
    pub fn frame_ns(&self) -> u64 {
        (1_000_000_000u64 * self.den.max(1) as u64) / self.num.max(1) as u64
    }

    pub fn parse(text: &str) -> Result<FrameRate> {
        let t = text.trim();
        if let Some((_, num)) = NTSC.iter().find(|(name, _)| *name == t || (*name == "23.976" && t == "23.98")) {
            return FrameRate::checked(*num, 1001);
        }
        if let Some((n, d)) = t.split_once('/') {
            if let (Ok(n), Ok(d)) = (n.trim().parse(), d.trim().parse()) {
                return FrameRate::checked(n, d);
            }
        }
        if let Ok(n) = t.parse::<i32>() {
            return FrameRate::checked(n, 1);
        }
        bail!("canvas fps `{t}` is not a frame rate. Give a whole number from 1 to 120, one of 23.976, 29.97, 59.94 and 119.88, or a fraction like \"30000/1001\"")
    }

    fn from_float(f: f64) -> Result<FrameRate> {
        if f.fract() == 0.0 {
            return FrameRate::checked(f as i32, 1);
        }
        match NTSC.iter().find(|(name, _)| (name.parse::<f64>().unwrap_or(0.0) - f).abs() < 0.006) {
            Some((_, num)) => FrameRate::checked(*num, 1001),
            None => FrameRate::parse(&f.to_string()),
        }
    }

    fn checked(num: i32, den: i32) -> Result<FrameRate> {
        let rate = FrameRate { num, den };
        if num <= 0 || den <= 0 || !(1.0..=120.0).contains(&rate.as_f64()) {
            bail!("canvas fps {num}/{den} is out of range: it must be from 1 to 120 frames a second");
        }
        Ok(rate)
    }
}

impl Default for FrameRate {
    fn default() -> Self {
        FrameRate::whole(30)
    }
}

impl std::fmt::Display for FrameRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match (self.den, NTSC.iter().find(|(_, n)| *n == self.num)) {
            (1, _) => write!(f, "{}", self.num),
            (1001, Some((name, _))) => f.write_str(name),
            _ => write!(f, "{}/{}", self.num, self.den),
        }
    }
}

impl Serialize for FrameRate {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        if self.den == 1 {
            s.serialize_i32(self.num)
        } else {
            s.serialize_str(&self.to_string())
        }
    }
}

impl<'de> Deserialize<'de> for FrameRate {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<FrameRate, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Written {
            Whole(i64),
            Float(f64),
            Text(String),
        }
        let rate = match Written::deserialize(d)? {
            Written::Whole(n) => FrameRate::checked(i32::try_from(n).unwrap_or(0), 1),
            Written::Float(f) => FrameRate::from_float(f),
            Written::Text(t) => FrameRate::parse(&t),
        };
        rate.map_err(serde::de::Error::custom)
    }
}

impl schemars::JsonSchema for FrameRate {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "FrameRate".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            // A number for a form and for `config.set`, which takes one type
            // a key. A config file may also give the rate as a string.
            "type": "number",
            "minimum": 1,
            "maximum": 120,
            "description": "Frames a second: a whole number from 1 to 120, or 23.976, 29.97, 59.94 or 119.88. A config file may also write a fraction as a string, \"30000/1001\".",
            "examples": [30, 25, 29.97, 59.94],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize, Serialize)]
    struct C {
        fps: FrameRate,
    }

    fn read(text: &str) -> Result<FrameRate, toml::de::Error> {
        toml::from_str::<C>(text).map(|c| c.fps)
    }

    #[test]
    fn every_way_of_writing_a_rate_reads() {
        assert_eq!(read("fps = 30").unwrap(), FrameRate::whole(30));
        assert_eq!(read("fps = 29.97").unwrap(), FrameRate { num: 30000, den: 1001 });
        assert_eq!(read("fps = \"59.94\"").unwrap(), FrameRate { num: 60000, den: 1001 });
        assert_eq!(read("fps = \"30000/1001\"").unwrap(), FrameRate { num: 30000, den: 1001 });
        assert_eq!(read("fps = 23.98").unwrap(), FrameRate { num: 24000, den: 1001 });
        assert_eq!(read("fps = 25.0").unwrap(), FrameRate::whole(25));
    }

    #[test]
    fn a_whole_rate_is_written_back_as_an_integer_and_ntsc_as_people_say_it() {
        assert_eq!(toml::to_string(&C { fps: FrameRate::whole(25) }).unwrap().trim(), "fps = 25");
        assert_eq!(toml::to_string(&C { fps: read("fps = 59.94").unwrap() }).unwrap().trim(), "fps = \"59.94\"");
    }

    #[test]
    fn the_arithmetic_uses_the_true_rate() {
        let r = read("fps = 29.97").unwrap();
        assert_eq!(r.rounded(), 30);
        assert_eq!(r.frame_ns(), 33_366_666);
    }

    #[test]
    fn a_bad_rate_says_what_is_accepted() {
        let e = read("fps = 0").unwrap_err().to_string();
        assert!(e.contains("from 1 to 120"), "{e}");
        let e = read("fps = \"fast\"").unwrap_err().to_string();
        assert!(e.contains("29.97") && e.contains("30000/1001"), "{e}");
    }
}
