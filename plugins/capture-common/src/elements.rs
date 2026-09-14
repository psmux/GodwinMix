//! Choosing an element that is actually installed, and setting a property it
//! may or may not have.
//!
//! Every capture plugin has a first choice per platform and at least one
//! fallback, usually because the first choice has an open bug. The pattern is
//! always the same: a list in preference order, and the first factory the
//! registry has wins. A machine with none of them gets one error naming every
//! candidate it looked for and the package that carries them, rather than a
//! parse failure naming only the first.

use gstreamer as gst;
use gstreamer::glib;
use gstreamer::prelude::*;

/// Is this factory installed on this machine?
pub fn exists(factory: &str) -> bool {
    gst::ElementFactory::find(factory).is_some()
}

/// The first of `candidates` that exists, in order.
pub fn first(candidates: &[&'static str]) -> Option<&'static str> {
    candidates.iter().copied().find(|f| exists(f))
}

/// The first of `candidates` that exists, or an error naming all of them.
///
/// `what` is what the operator was trying to do, so the message reads as a
/// sentence: "no element for capturing a camera on this machine".
pub fn require(
    what: &str,
    candidates: &[&'static str],
    hint: &str,
) -> Result<&'static str, String> {
    first(candidates).ok_or_else(|| {
        format!(
            "this machine has no GStreamer element for {what}. Looked for: {}. {hint}",
            candidates.join(", ")
        )
    })
}

/// Set a property only if the element has one by that name, converting the
/// value to whatever type the property actually is.
///
/// Capture sources disagree about which knobs they expose, the set changes
/// between GStreamer releases, and the same idea is an `int` on one element
/// and an `int64` on the next. Setting a property an element does not have
/// aborts the process in the bindings, so every optional knob goes through
/// here. Returns whether it landed.
pub fn set_if_present(element: &gst::Element, name: &str, value: &glib::Value) -> bool {
    let Some(spec) = element.find_property(name) else {
        return false;
    };
    if spec.value_type() == value.type_() {
        element.set_property_from_value(name, value);
        return true;
    }
    match value.transform_with_type(spec.value_type()) {
        Ok(converted) => {
            element.set_property_from_value(name, &converted);
            true
        }
        Err(_) => false,
    }
}

/// `set_if_present` for a whole number, whatever width the property is.
pub fn set_number(element: &gst::Element, name: &str, value: i64) -> bool {
    set_if_present(element, name, &value.to_value())
}

/// `set_if_present` for a flag.
pub fn set_flag(element: &gst::Element, name: &str, value: bool) -> bool {
    set_if_present(element, name, &value.to_value())
}

/// `set_if_present` for a string.
pub fn set_text(element: &gst::Element, name: &str, value: &str) -> bool {
    set_if_present(element, name, &value.to_value())
}

/// Point a source element at a device, whatever it calls the property.
///
/// The first choice is always `devices::find`, which lets GStreamer's own
/// device provider configure the element. This is the fallback for the case
/// that provider cannot serve: an operator who forced a particular element in
/// the settings, or a platform whose provider is not installed.
///
/// `id` may be a path, a name or an index; each property is offered the value
/// and the ones whose type it does not fit decline. Returns the property that
/// took it.
pub fn point_at(element: &gst::Element, id: &str) -> Option<&'static str> {
    if id.is_empty() {
        return None;
    }
    [
        "device-path",
        "device",
        "path",
        "device-name",
        "device-index",
    ]
    .into_iter()
    .find(|name| set_text(element, name, id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gst() {
        gst::init().expect("GStreamer starts");
    }

    #[test]
    fn a_core_element_is_found_and_a_made_up_one_is_not() {
        gst();
        assert!(exists("videoconvert"));
        assert!(!exists("no-such-element-anywhere"));
    }

    #[test]
    fn the_first_installed_candidate_wins() {
        gst();
        assert_eq!(
            first(&["no-such-element-anywhere", "videoconvert", "videoscale"]),
            Some("videoconvert")
        );
        assert_eq!(first(&["no-such-element-anywhere"]), None);
    }

    #[test]
    fn nothing_installed_names_every_candidate_it_looked_for() {
        gst();
        let err = require(
            "capturing a unicorn",
            &["unicornsrc", "pegasussrc"],
            "Install one.",
        )
        .expect_err("neither exists");
        assert!(err.contains("unicornsrc"), "{err}");
        assert!(err.contains("pegasussrc"), "{err}");
        assert!(err.contains("Install one."), "{err}");
    }

    #[test]
    fn an_optional_property_is_set_only_when_the_element_has_it() {
        gst();
        let e = gst::ElementFactory::make("videotestsrc")
            .build()
            .expect("videotestsrc");
        assert!(set_flag(&e, "is-live", true));
        assert!(!set_flag(&e, "there-is-no-such-property", true));
    }

    #[test]
    fn an_element_with_no_device_property_declines_and_says_nothing_landed() {
        gst();
        let e = gst::ElementFactory::make("filesrc")
            .build()
            .expect("filesrc");
        assert_eq!(point_at(&e, "/dev/video0"), None);
        assert_eq!(point_at(&e, ""), None);
    }

    #[test]
    fn a_number_lands_in_a_property_of_a_different_width() {
        gst();
        let e = gst::ElementFactory::make("queue").build().expect("queue");
        // max-size-time is a guint64; the caller only ever has an i64.
        assert!(set_number(&e, "max-size-time", 200_000_000));
        assert_eq!(e.property::<u64>("max-size-time"), 200_000_000);
    }
}
