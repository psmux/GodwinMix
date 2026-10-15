//! What the renderer tells the sites it visits: who it is, and where an
//! embedded player is shown from.

use cef::*;

/// The app's identifier, sent as the referrer of an embedded YouTube player.
/// YouTube refuses to play an embed that names no embedder ("Video player
/// configuration error, Error 153"), and its own guidance for an app with no
/// web origin is the app's identifier in this form.
pub const EMBEDDER: &str = "https://mix.godwin.desktop/";

/// The user agent: Chrome's own, with this renderer's name on the end.
///
/// It used to be Chromium's default with the product replaced, which left no
/// `Chrome/` in it at all. Sites that check for a browser they know then
/// treat the page as an unknown one: a sign in page refuses it, a web app
/// asks for a supported browser. Chrome's string with `GodwinMix/` after it
/// keeps the name a broadcast aware page looks for and reads as Chrome to
/// everything else, which is what this is.
pub fn user_agent() -> String {
    let platform = if cfg!(target_os = "windows") {
        "Windows NT 10.0; Win64; x64"
    } else if cfg!(target_os = "macos") {
        "Macintosh; Intel Mac OS X 10_15_7"
    } else {
        "X11; Linux x86_64"
    };
    format!(
        "Mozilla/5.0 ({platform}) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{}.0.0.0 Safari/537.36 GodwinMix/{}",
        sys::CHROME_VERSION_MAJOR,
        env!("CARGO_PKG_VERSION")
    )
}

/// Whether this is YouTube's embedded player, which needs a referrer.
pub fn youtube_embed(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    let rest = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://")).unwrap_or(&lower);
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let path = &rest[host.len()..];
    matches!(host, "www.youtube.com" | "youtube.com" | "m.youtube.com" | "www.youtube-nocookie.com" | "youtube-nocookie.com")
        && path.starts_with("/embed/")
}

/// An embedded player told to start by itself, as a source has to: nobody
/// is there to press play. An address that already says whether to autoplay
/// is left as it is.
pub fn autoplaying(url: &str) -> String {
    if !youtube_embed(url) || url.contains("autoplay=") {
        return url.to_string();
    }
    let (base, frag) = match url.split_once('#') {
        Some((b, f)) => (b, format!("#{f}")),
        None => (url, String::new()),
    };
    let sep = if base.contains('?') { '&' } else { '?' };
    format!("{base}{sep}autoplay=1{frag}")
}

/// Load `url` in the main frame. An embed is told to autoplay, and
/// `Requests` gives it its referrer on the way out.
pub fn load(browser: &Browser, url: &str) {
    if let Some(frame) = browser.main_frame() {
        frame.load_url(Some(&CefString::from(autoplaying(url).as_str())));
    }
}

// Puts the referrer on a navigation to an embedded player. A request made by
// hand with `Frame::load_request` cannot do it: Chromium refuses one whose
// origin the frame has not been to.
wrap_request_handler! {
    pub struct Requests;

    impl RequestHandler {
        fn resource_request_handler(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            is_navigation: ::std::os::raw::c_int,
            _is_download: ::std::os::raw::c_int,
            _request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut ::std::os::raw::c_int>,
        ) -> Option<ResourceRequestHandler> {
            let url = request.map(|r| CefString::from(&r.url()).to_string()).unwrap_or_default();
            (is_navigation == 1 && youtube_embed(&url)).then(EmbedReferrer::new)
        }
    }
}

wrap_resource_request_handler! {
    struct EmbedReferrer;

    impl ResourceRequestHandler {
        fn on_before_resource_load(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            if let Some(r) = request {
                r.set_referrer(
                    Some(&CefString::from(EMBEDDER)),
                    ReferrerPolicy::from(sys::cef_referrer_policy_t::REFERRER_POLICY_NEVER_CLEAR_REFERRER),
                );
            }
            ReturnValue::from(sys::cef_return_value_t::RV_CONTINUE)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_agent_reads_as_chrome_and_names_the_renderer() {
        let ua = user_agent();
        assert!(ua.contains(&format!("Chrome/{}.0.0.0", sys::CHROME_VERSION_MAJOR)), "{ua}");
        assert!(ua.ends_with(concat!("GodwinMix/", env!("CARGO_PKG_VERSION"))), "{ua}");
    }

    #[test]
    fn embeds_are_told_apart_from_watch_pages() {
        assert!(youtube_embed("https://www.youtube.com/embed/aqz-KE-bpKQ"));
        assert!(youtube_embed("https://www.youtube-nocookie.com/embed/x?start=3"));
        assert!(!youtube_embed("https://www.youtube.com/watch?v=aqz-KE-bpKQ"));
        assert!(!youtube_embed("https://youtu.be/aqz-KE-bpKQ"));
        assert!(!youtube_embed("https://example.com/embed/youtube.com"));
    }

    #[test]
    fn embeds_autoplay_unless_told_otherwise() {
        assert_eq!(autoplaying("https://www.youtube.com/embed/x"), "https://www.youtube.com/embed/x?autoplay=1");
        assert_eq!(autoplaying("https://www.youtube.com/embed/x?start=5#t"), "https://www.youtube.com/embed/x?start=5&autoplay=1#t");
        assert_eq!(autoplaying("https://www.youtube.com/embed/x?autoplay=0"), "https://www.youtube.com/embed/x?autoplay=0");
        assert_eq!(autoplaying("https://example.com/"), "https://example.com/");
    }
}
