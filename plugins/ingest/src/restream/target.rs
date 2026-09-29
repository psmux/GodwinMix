//! Where a destination sends, taken apart, and the sentences its failures
//! become.

use godwinmix_protocol::destination::{platform, uri_host, RetryPolicy};

/// One destination, as the restreamer needs it.
#[derive(Debug, Clone)]
pub struct Target {
    /// The destination's id, for log lines.
    pub id: String,
    /// The platform id from the table, or anything else for a custom one.
    pub platform: String,
    /// The whole address, key and all.
    pub url: String,
    pub policy: RetryPolicy,
    /// Bytes held for a slow far end before GOPs are dropped.
    pub queue_bytes: usize,
}

impl Target {
    /// A target with the platform's own retry policy and a queue of about
    /// five seconds at 6 Mbit/s.
    pub fn new(id: &str, platform_id: &str, url: &str) -> Target {
        Target {
            id: id.to_string(),
            platform: platform_id.to_string(),
            url: url.trim().to_string(),
            policy: platform(platform_id).map(|p| p.policy).unwrap_or(RetryPolicy::Own),
            queue_bytes: 4 * 1024 * 1024,
        }
    }

    /// What a sentence calls the far end: "YouTube", or the address with no
    /// key for one not on the table.
    pub fn name(&self) -> String {
        match platform(&self.platform) {
            Some(p) if !p.server.is_empty() => p.title.to_string(),
            _ => uri_host(&self.url),
        }
    }
}

/// An RTMP address, taken apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RtmpUrl {
    pub tls: bool,
    pub host: String,
    pub port: u16,
    pub app: String,
    /// The stream key and whatever query rides on it, as the server wants it.
    pub stream: String,
}

impl RtmpUrl {
    /// `rtmp://host[:port]/app/stream`, or `rtmps://`. The first path
    /// segment is the application; the rest, query and all, is the stream.
    pub fn parse(url: &str) -> Result<RtmpUrl, String> {
        let (scheme, rest) = url
            .split_once("://")
            .ok_or_else(|| format!("'{}' is not an address: it needs rtmp:// in front", uri_host(url)))?;
        let tls = match scheme.to_ascii_lowercase().as_str() {
            "rtmp" => false,
            "rtmps" => true,
            other => return Err(format!("{other}:// is not RTMP. Use rtmp:// or rtmps://")),
        };
        let (hostport, path) = rest.split_once('/').unwrap_or((rest, ""));
        let (host, port) = match hostport.rsplit_once(':') {
            Some((h, p)) => (h, p.parse().map_err(|_| format!("'{p}' is not a port number"))?),
            None => (hostport, if tls { 443 } else { 1935 }),
        };
        let (app, stream) = path.split_once('/').unwrap_or((path, ""));
        if host.is_empty() {
            return Err("the address has no server name in it".into());
        }
        if app.is_empty() || stream.is_empty() {
            return Err(format!(
                "{scheme}://{hostport} needs an application and a stream key after it, as in \
                 {scheme}://{hostport}/live/<key>"
            ));
        }
        Ok(RtmpUrl {
            tls,
            host: host.to_string(),
            port,
            app: app.to_string(),
            stream: stream.to_string(),
        })
    }

    /// What goes in `tcUrl`, and what a sentence may show: no key.
    pub fn tc_url(&self) -> String {
        let scheme = if self.tls { "rtmps" } else { "rtmp" };
        format!("{scheme}://{}:{}/{}", self.host, self.port, self.app)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_platform_servers_take_apart_into_app_and_key() {
        let yt = RtmpUrl::parse("rtmp://a.rtmp.youtube.com/live2/abcd-efgh").unwrap();
        assert_eq!((yt.port, yt.app.as_str(), yt.stream.as_str()), (1935, "live2", "abcd-efgh"));
        let fb = RtmpUrl::parse("rtmps://live-api-s.facebook.com:443/rtmp/FB-1?s_bl=1&a=b").unwrap();
        assert!(fb.tls);
        assert_eq!(fb.stream, "FB-1?s_bl=1&a=b");
        assert_eq!(fb.tc_url(), "rtmps://live-api-s.facebook.com:443/rtmp");
    }

    #[test]
    fn an_address_with_no_key_says_what_it_needs() {
        let err = RtmpUrl::parse("rtmp://live.twitch.tv/app").unwrap_err();
        assert!(err.contains("/live/<key>"), "{err}");
        assert!(RtmpUrl::parse("http://x/y/z").unwrap_err().contains("not RTMP"));
    }

    #[test]
    fn a_sentence_names_the_platform_or_the_host_and_never_the_key() {
        let t = Target::new("yt", "youtube", "rtmp://a.rtmp.youtube.com/live2/secret");
        assert_eq!(t.name(), "YouTube");
        assert_eq!(t.policy, RetryPolicy::Cdn);
        let c = Target::new("mine", "custom", "rtmp://10.0.0.9:1935/live/secret");
        assert_eq!(c.name(), "rtmp://10.0.0.9:1935");
    }
}
