//! A web page source, driven by hand: `source.page.input` and
//! `source.page.navigate`.
//!
//! An operator clicks, scrolls and types into a page the mixer is rendering,
//! the way they would in a browser. Each event names a point as a share of the
//! page, 0.0 at the left or top edge and 1.0 at the right or bottom, so a
//! client maps from whatever size it shows the picture at and never needs to
//! know the size the page is drawn at. The renderer turns the shares into its
//! own pixels.
//!
//! Every event is one JSON line on the renderer's control channel, written
//! exactly as it is serialised here, which is why the tag is `type` and the
//! fields are flat.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The most events one call may carry. A client sends what it gathered since
/// its last call, which at pointer rate is a handful.
pub const MAX_EVENTS: usize = 64;

/// The most text one `text` event may carry, in characters.
pub const MAX_TEXT: usize = 2000;

/// A mouse button.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PageButton {
    #[default]
    Left,
    Middle,
    Right,
}

/// The keys held down with an event.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PageModifiers {
    #[serde(default, skip_serializing_if = "is_false")]
    pub shift: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub ctrl: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub alt: bool,
    /// Command on a Mac, the Windows key elsewhere.
    #[serde(default, skip_serializing_if = "is_false")]
    pub meta: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// One thing the operator did to the page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PageEvent {
    /// The pointer moved to a point. Moves while a button is held are drags.
    Move {
        x: f64,
        y: f64,
        #[serde(default, flatten)]
        mods: PageModifiers,
    },
    /// A button went down at a point. `count` is 2 for the second press of a
    /// double click.
    Down {
        x: f64,
        y: f64,
        #[serde(default)]
        button: PageButton,
        #[serde(default = "one")]
        count: u8,
        #[serde(default, flatten)]
        mods: PageModifiers,
    },
    /// A button came up at a point.
    Up {
        x: f64,
        y: f64,
        #[serde(default)]
        button: PageButton,
        #[serde(default = "one")]
        count: u8,
        #[serde(default, flatten)]
        mods: PageModifiers,
    },
    /// A scroll at a point, in CSS pixels the way a browser's `wheel` event
    /// gives them: a positive `dy` scrolls down the page.
    Wheel {
        x: f64,
        y: f64,
        #[serde(default)]
        dx: f64,
        #[serde(default)]
        dy: f64,
    },
    /// The pointer left the picture.
    Leave,
    /// A key pressed and let go. `code` is the key code a browser's
    /// `keydown` gives as `keyCode` (13 for Enter, 37 for the left arrow, 65
    /// for A). `key` is the character it typed, when it typed one.
    Key {
        code: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        #[serde(default, flatten)]
        mods: PageModifiers,
    },
    /// Text typed into whatever has focus on the page, as one piece. What a
    /// phone keyboard or an input method hands over.
    Text { text: String },
}

fn one() -> u8 {
    1
}

impl PageEvent {
    /// The line the renderer reads.
    pub fn line(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Why this event cannot be sent, if it cannot.
    pub fn check(&self) -> Result<(), String> {
        let point = |x: f64, y: f64| {
            if (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y) {
                Ok(())
            } else {
                Err(format!(
                    "x and y are shares of the page from 0.0 to 1.0, and this event has x {x} and y {y}. \
                     Divide the point by the size the picture is shown at"
                ))
            }
        };
        match self {
            PageEvent::Move { x, y, .. } | PageEvent::Down { x, y, .. } | PageEvent::Up { x, y, .. } => point(*x, *y),
            PageEvent::Wheel { x, y, dx, dy } => {
                point(*x, *y)?;
                if dx.abs() > 10_000.0 || dy.abs() > 10_000.0 || !dx.is_finite() || !dy.is_finite() {
                    return Err(format!("a scroll moves at most 10000 pixels at a time, and this one is {dx} by {dy}"));
                }
                Ok(())
            }
            PageEvent::Text { text } if text.chars().count() > MAX_TEXT => {
                Err(format!("one text event carries at most {MAX_TEXT} characters; send the rest in another"))
            }
            _ => Ok(()),
        }
    }
}

/// `source.page.input`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PageInputRequest {
    /// Source id. A web page source drawn by the browser renderer.
    pub id: String,
    /// What the operator did, oldest first.
    pub events: Vec<PageEvent>,
}

/// What `source.page.input` answers with.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct PageInputResult {
    pub id: String,
    /// Events handed to the renderer.
    pub sent: usize,
    /// Events dropped because the renderer was not keeping up. Pointer moves
    /// are the usual ones, and the next move puts the pointer right.
    pub dropped: usize,
}

/// Where `source.page.navigate` goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PageAction {
    /// To `url`. Kept as the source's address, so a saved project opens it.
    #[default]
    Go,
    Back,
    Forward,
    Reload,
}

/// `source.page.navigate`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PageNavigateRequest {
    /// Source id.
    pub id: String,
    #[serde(default)]
    pub action: PageAction,
    /// The address for `go`: http or https. A bare host such as
    /// `example.com` is taken as https.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Where a page is, as its renderer last said.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PageState {
    /// The address the page is showing now, after any link the operator
    /// followed.
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub can_back: bool,
    #[serde(default)]
    pub can_forward: bool,
    #[serde(default)]
    pub loading: bool,
}

/// What `source.page.navigate` answers with.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct PageNavigateResult {
    pub id: String,
    pub action: PageAction,
    /// The address the source keeps. For `go` the new one; otherwise the one
    /// it had, since back, forward and reload are not saved.
    pub url: String,
}

/// The line that asks the renderer to go somewhere.
pub fn navigate_line(action: PageAction, url: Option<&str>) -> String {
    match action {
        PageAction::Go => serde_json::json!({ "type": "go", "url": url.unwrap_or_default() }).to_string(),
        PageAction::Back => r#"{"type":"back"}"#.into(),
        PageAction::Forward => r#"{"type":"forward"}"#.into(),
        PageAction::Reload => r#"{"type":"reload"}"#.into(),
    }
}

/// An address an operator typed, made into one a browser loads: a bare host
/// gets https in front. Anything that is not http or https is refused, since
/// a `file:` or `javascript:` address typed into a shared desk is a way into
/// the mixer's machine.
pub fn typed_url(typed: &str) -> Result<String, String> {
    let t = typed.trim();
    if t.is_empty() {
        return Err("the address is empty; type one such as https://example.com".into());
    }
    let lower = t.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return Ok(t.to_string());
    }
    if lower.contains("://") || lower.starts_with("javascript:") || lower.starts_with("file:") || lower.starts_with("data:") {
        return Err(format!("a page source opens http and https addresses only, and `{t}` is neither"));
    }
    if t.contains(char::is_whitespace) || !t.contains('.') {
        return Err(format!("`{t}` does not look like an address; type one such as https://example.com"));
    }
    Ok(format!("https://{t}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_are_the_lines_the_renderer_reads() {
        let down = PageEvent::Down { x: 0.5, y: 0.25, button: PageButton::Left, count: 2, mods: PageModifiers::default() };
        assert_eq!(down.line(), r#"{"type":"down","x":0.5,"y":0.25,"button":"left","count":2}"#);
        let key = PageEvent::Key { code: 13, key: None, mods: PageModifiers { shift: true, ..Default::default() } };
        assert_eq!(key.line(), r#"{"type":"key","code":13,"shift":true}"#);
        assert_eq!(PageEvent::Leave.line(), r#"{"type":"leave"}"#);
        let text = PageEvent::Text { text: "héllo \"q\"\n".into() };
        assert_eq!(serde_json::from_str::<PageEvent>(&text.line()).unwrap(), text);
    }

    #[test]
    fn a_client_may_leave_out_the_defaults() {
        let e: PageEvent = serde_json::from_str(r#"{"type":"down","x":0.1,"y":0.2}"#).unwrap();
        assert_eq!(e, PageEvent::Down { x: 0.1, y: 0.2, button: PageButton::Left, count: 1, mods: PageModifiers::default() });
        let e: PageEvent = serde_json::from_str(r#"{"type":"key","code":65,"key":"a","ctrl":true}"#).unwrap();
        assert!(matches!(e, PageEvent::Key { code: 65, mods: PageModifiers { ctrl: true, .. }, .. }));
    }

    #[test]
    fn a_point_off_the_page_is_refused_with_the_rule() {
        let e = PageEvent::Move { x: 640.0, y: 0.5, mods: PageModifiers::default() };
        assert!(e.check().unwrap_err().contains("0.0 to 1.0"));
        assert!(PageEvent::Move { x: 1.0, y: 0.0, mods: PageModifiers::default() }.check().is_ok());
        let w = PageEvent::Wheel { x: 0.5, y: 0.5, dx: 0.0, dy: f64::INFINITY };
        assert!(w.check().is_err());
    }

    #[test]
    fn navigation_lines() {
        assert_eq!(navigate_line(PageAction::Back, None), r#"{"type":"back"}"#);
        assert_eq!(navigate_line(PageAction::Go, Some("https://a.b/?q=\"x\"")), r#"{"type":"go","url":"https://a.b/?q=\"x\""}"#);
    }

    #[test]
    fn typed_addresses() {
        assert_eq!(typed_url(" example.com/x ").unwrap(), "https://example.com/x");
        assert_eq!(typed_url("http://a.b").unwrap(), "http://a.b");
        assert!(typed_url("file:///etc/passwd").is_err());
        assert!(typed_url("javascript:alert(1)").is_err());
        assert!(typed_url("hello world").is_err());
        assert!(typed_url("").is_err());
    }
}
