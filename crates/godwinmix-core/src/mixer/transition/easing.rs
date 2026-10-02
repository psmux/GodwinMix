//! How progress is shaped over a transition.
//!
//! Four curves and no more, because four is what an operator can tell apart
//! by eye. Each maps 0 to 0 and 1 to 1, so an eased transition still starts
//! where it was and ends where it was going; only the frames between differ.

/// The shape of a transition's progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Easing {
    /// The same distance every frame.
    Linear,
    /// Slow away, fast in.
    In,
    /// Fast away, slow in. What a lower third sliding on wants.
    Out,
    /// Slow at both ends. The default, and the curve `fade` has always had.
    #[default]
    InOut,
}

impl Easing {
    /// Read a name. Anything this build does not know is the default: the
    /// control layer has already refused a name that is wrong, so what gets
    /// here unchecked is a stored document from an older build.
    pub fn parse(name: Option<&str>) -> Easing {
        match name.map(|s| s.trim().to_lowercase()).as_deref() {
            Some("linear") => Easing::Linear,
            Some("ease-in") | Some("in") => Easing::In,
            Some("ease-out") | Some("out") => Easing::Out,
            _ => Easing::InOut,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Easing::Linear => "linear",
            Easing::In => "ease-in",
            Easing::Out => "ease-out",
            Easing::InOut => "ease-in-out",
        }
    }

    /// Progress at `t`, both from 0 to 1.
    ///
    /// Cubic in and out, and the smoothstep `fade` has always used for in and
    /// out together, so a fade taken without an easing is the fade it was.
    pub fn at(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::In => t * t * t,
            Easing::Out => 1.0 - (1.0 - t).powi(3),
            Easing::InOut => t * t * (3.0 - 2.0 * t),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_easing_starts_at_nothing_and_ends_at_everything() {
        for e in [Easing::Linear, Easing::In, Easing::Out, Easing::InOut] {
            assert_eq!(e.at(0.0), 0.0, "{e:?}");
            assert_eq!(e.at(1.0), 1.0, "{e:?}");
            assert_eq!(Easing::parse(Some(e.name())), e);
        }
        assert!(Easing::In.at(0.5) < 0.5 && Easing::Out.at(0.5) > 0.5);
        assert_eq!(Easing::InOut.at(0.5), 0.5);
        assert_eq!(Easing::parse(Some("bounce")), Easing::InOut);
    }
}
