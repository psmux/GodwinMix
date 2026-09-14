//! `gmx support-bundle`: one file an issue can be filed with.
//!
//! Versions, the doctor's verdict, the config with every secret taken out,
//! every pipeline's graph, the last hour of the session log and the log files.
//! The point is that the person reporting a fault does not have to be told
//! what to collect, and nothing they collect leaks their stream key.
//!
//! The zip is written by `godwinmix_core::zip`, a store only writer with a
//! reader beside it. It started here and moved when a collection export wanted
//! the same hundred lines; the reasoning is unchanged, and it is in that
//! module's head.

use anyhow::{Context, Result};
use godwinmix_core::zip::{crc32, Zip};
use std::path::PathBuf;

// --- redaction ---------------------------------------------------------------

/// Keys whose value never leaves the machine.
const SECRET_KEYS: &[&str] = &["token", "secret", "password", "passwd", "key", "auth", "cookie"];

/// The config with everything that authorises anybody replaced.
///
/// Two kinds of secret: a value under a key that says what it is, and a stream
/// key sitting in the middle of a URL where nothing says what it is. Both are
/// replaced, and the URL keeps its host and its shape so the reader can still
/// see that the output was pointed at YouTube and not at Twitch.
pub fn redact_config(text: &str) -> String {
    let Ok(value) = toml::from_str::<toml::Value>(text) else {
        // An unparseable config is itself a thing worth having in the bundle,
        // but not at the price of shipping a token in it.
        return "# this config did not parse, so it is left out rather than \
                shipped unredacted\n"
            .to_string();
    };
    let mut value = value;
    redact_value(&mut value, "");
    toml::to_string_pretty(&value).unwrap_or_else(|e| format!("# could not be rewritten: {e}\n"))
}

fn redact_value(value: &mut toml::Value, key: &str) {
    match value {
        toml::Value::Table(table) => {
            for (k, v) in table.iter_mut() {
                redact_value(v, k);
            }
        }
        toml::Value::Array(items) => {
            for item in items.iter_mut() {
                redact_value(item, key);
            }
        }
        toml::Value::String(s) => {
            let lower = key.to_ascii_lowercase();
            if SECRET_KEYS.iter().any(|needle| lower.contains(needle)) {
                *s = "REDACTED".to_string();
            } else if s.contains("://") {
                *s = redact_uri(s);
            }
        }
        _ => {}
    }
}

/// A URL with its credentials, its query string and its last path segment
/// taken out, because on every streaming service the last path segment is the
/// stream key.
pub fn redact_uri(uri: &str) -> String {
    let Some((scheme, rest)) = uri.split_once("://") else { return uri.to_string() };
    let (rest, _query) = rest.split_once('?').unwrap_or((rest, ""));
    let had_query = uri.contains('?');
    // user:password@host
    let rest = match rest.split_once('@') {
        Some((_creds, host)) => format!("REDACTED@{host}"),
        None => rest.to_string(),
    };
    let mut parts: Vec<&str> = rest.split('/').collect();
    if parts.len() > 2 {
        // rtmp://host/app/streamkey keeps the host and the app.
        if let Some(last) = parts.last_mut() {
            if !last.is_empty() {
                *last = "REDACTED";
            }
        }
    }
    let mut out = format!("{scheme}://{}", parts.join("/"));
    if had_query {
        out.push_str("?REDACTED");
    }
    out
}

// --- the bundle --------------------------------------------------------------

/// What to put in, and where to get it.
pub struct BundleOptions {
    pub config_path: PathBuf,
    pub runtime_dir: PathBuf,
    /// A running mixer to ask for the things only it knows: the pipeline
    /// graphs, the metrics, the doctor's verdict. `None` builds an offline
    /// bundle from the files alone, which is what somebody whose mixer has
    /// already died has.
    pub url: Option<String>,
    pub token: Option<String>,
    pub out: PathBuf,
}

/// Build the bundle and write it. Answers the path and what went in.
pub async fn build(options: &BundleOptions) -> Result<(PathBuf, Vec<String>)> {
    let mut zip = Zip::new();
    let mut included = Vec::new();
    let add = |zip: &mut Zip, name: &str, data: Vec<u8>, included: &mut Vec<String>| {
        if zip.add(name, &data) {
            included.push(format!("{name} ({} bytes)", data.len()));
        }
    };

    add(&mut zip, "versions.txt", versions().into_bytes(), &mut included);

    if let Ok(text) = std::fs::read_to_string(&options.config_path) {
        add(&mut zip, "config.redacted.toml", redact_config(&text).into_bytes(), &mut included);
    }

    // The doctor runs locally whether or not a mixer is up: its answers are
    // about the machine.
    gstreamer::init().ok();
    let checks = godwinmix_core::observe::doctor::run(&options.config_path);
    add(&mut zip, "doctor.txt", godwinmix_core::observe::doctor::format(&checks).into_bytes(), &mut included);

    // A running mixer answers with its own last hour, which is authoritative
    // and is fetched below. Without one, the file on disk is what there is.
    if options.url.is_none() {
        let session = godwinmix_core::observe::session::session().tail_since(3600).join("\n");
        let session = if session.is_empty() {
            std::fs::read_to_string(godwinmix_core::observe::session::path_in(&options.runtime_dir))
                .map(|text| tail_lines(&text, 20_000))
                .unwrap_or_default()
        } else {
            session
        };
        if !session.is_empty() {
            add(&mut zip, "session-last-hour.jsonl", session.into_bytes(), &mut included);
        }
    }

    for path in godwinmix_core::observe::logs::log_files(&options.runtime_dir) {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        // Ten megabytes of each, from the end, which is the part that matters.
        let cut = bytes.len().saturating_sub(10 * 1024 * 1024);
        add(&mut zip, &format!("logs/{name}"), bytes[cut..].to_vec(), &mut included);
    }

    add(&mut zip, "levels.json", godwinmix_core::observe::logs::levels().to_string().into_bytes(), &mut included);

    if let Some(url) = &options.url {
        for (name, data) in from_running_mixer(url, options.token.as_deref()).await {
            add(&mut zip, &name, data, &mut included);
        }
    } else {
        // In process, which is what a bundle taken by the core itself has.
        for name in godwinmix_core::observe::introspect::names() {
            if let Ok(dot) = godwinmix_core::observe::introspect::dot(&name) {
                add(&mut zip, &format!("dot/{name}.dot"), dot.into_bytes(), &mut included);
            }
        }
        add(&mut zip, "metrics.txt", godwinmix_core::observe::metrics::render().into_bytes(), &mut included);
    }

    let bytes = zip.finish();
    if let Some(dir) = options.out.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir).ok();
        }
    }
    std::fs::write(&options.out, &bytes)
        .with_context(|| format!("writing {}", options.out.display()))?;
    Ok((options.out.clone(), included))
}

/// Everything only a running mixer can answer. Each is optional: a mixer that
/// is half up still produces a bundle with whatever it managed.
async fn from_running_mixer(url: &str, token: Option<&str>) -> Vec<(String, Vec<u8>)> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();
    let get = |path: String| {
        let mut req = client.get(format!("{}{path}", url.trim_end_matches('/')));
        if let Some(token) = token {
            req = req.bearer_auth(token);
        }
        req.send()
    };

    let mut out = Vec::new();
    for (name, path) in [
        ("metrics.txt", "/metrics".to_string()),
        ("status.json", "/api/status".to_string()),
        ("startup-report.json", "/api/v1/core/startup_report".to_string()),
        ("pipeline-clock.json", "/api/v1/pipeline/clock".to_string()),
        ("session-last-hour.jsonl", "/api/v1/core/session_log?secs=3600".to_string()),
        // What plugins are installed, what they registered and what each one
        // is costing. The first question about a mixer that is misbehaving
        // with plugins on it is which plugin, and this is the answer.
        ("plugins.json", "/api/v1/plugins".to_string()),
        ("plugin-stats.json", "/api/v1/plugin/stats".to_string()),
    ] {
        if let Ok(response) = get(path).await {
            if let Ok(bytes) = response.bytes().await {
                out.push((name.to_string(), bytes.to_vec()));
            }
        }
    }

    // Every pipeline, by name, then a dot, a latency and a queue report each.
    let names: Vec<String> = match get("/api/v1/pipeline/list".into()).await {
        Ok(r) => r
            .json::<serde_json::Value>()
            .await
            .ok()
            .and_then(|v| v["pipelines"].as_array().cloned())
            .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    for name in names {
        for (dir, path) in [
            ("dot", format!("/api/v1/pipeline/dot?name={name}")),
            ("latency", format!("/api/v1/pipeline/latency?name={name}")),
            ("queues", format!("/api/v1/pipeline/queues?name={name}")),
        ] {
            if let Ok(response) = get(path).await {
                if let Ok(bytes) = response.bytes().await {
                    let ext = if dir == "dot" { "dot" } else { "json" };
                    out.push((format!("{dir}/{name}.{ext}"), bytes.to_vec()));
                }
            }
        }
    }
    out.extend(plugin_crash_reports(&get).await);
    out
}

/// Every plugin's own log and crash report.
///
/// The SDK writes a report to `<plugin root>/crash-<ts>.txt` when a plugin
/// dies of something it did not expect, and the core attaches the path to
/// `event/plugin.state {state: "failed", detail}`. A bundle taken after a
/// crash therefore carries the backtrace without anybody having to go looking
/// for it, which is the whole point of the file an issue template asks for.
async fn plugin_crash_reports<F, Fut>(get: &F) -> Vec<(String, Vec<u8>)>
where
    F: Fn(String) -> Fut,
    Fut: std::future::Future<Output = reqwest::Result<reqwest::Response>>,
{
    let mut out = Vec::new();
    let Ok(response) = get("/api/v1/plugins".to_string()).await else { return out };
    let Ok(listing) = response.json::<serde_json::Value>().await else { return out };
    for plugin in listing["plugins"].as_array().cloned().unwrap_or_default() {
        let (Some(name), Some(root)) = (plugin["name"].as_str(), plugin["root"].as_str()) else {
            continue;
        };
        let Ok(entries) = std::fs::read_dir(root) else { continue };
        for entry in entries.flatten() {
            let file = entry.file_name().to_string_lossy().into_owned();
            if !file.starts_with("crash-") {
                continue;
            }
            if let Ok(bytes) = std::fs::read(entry.path()) {
                // Capped like every other log in the bundle: a crash report
                // with a very long backtrace must not make the archive
                // unusable.
                let tail = tail_lines(&String::from_utf8_lossy(&bytes), 500);
                out.push((format!("plugins/{name}/{file}"), tail.into_bytes()));
            }
        }
    }
    out
}

fn versions() -> String {
    let (major, minor, micro, nano) = gstreamer::version();
    format!(
        "godwinmix {}\ngstreamer {major}.{minor}.{micro}.{nano}\nos {} {}\ntaken {}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
        godwinmix_core::observe::logs::rfc3339(&std::time::SystemTime::now()),
    )
}

fn tail_lines(text: &str, max: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(max);
    lines[start..].join("\n")
}

/// A name for today's bundle that sorts and does not collide.
pub fn default_name() -> String {
    let ts = godwinmix_core::observe::logs::rfc3339(&std::time::SystemTime::now())
        .replace([':', '.'], "-")
        .replace('Z', "");
    format!("godwinmix-support-{ts}.zip")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_archive_is_a_zip_every_tool_can_read() {
        let mut zip = Zip::new();
        zip.add("versions.txt", b"godwinmix 0.2.0\n");
        zip.add("logs/godwinmix.log", b"{\"level\":\"info\"}\n");
        let bytes = zip.finish();
        assert_eq!(&bytes[..4], b"PK\x03\x04", "no local header");
        // The end of central directory record, last twenty two bytes.
        let end = &bytes[bytes.len() - 22..];
        assert_eq!(&end[..4], b"PK\x05\x06");
        assert_eq!(u16::from_le_bytes([end[10], end[11]]), 2, "two entries");
        assert!(bytes.windows(4).any(|w| w == b"PK\x01\x02"), "no central directory");
    }

    #[test]
    fn a_name_already_in_the_archive_is_refused_rather_than_written_twice() {
        let mut zip = Zip::new();
        assert!(zip.add("a.txt", b"first"));
        assert!(!zip.add("a.txt", b"second"));
        let bytes = zip.finish();
        let end = &bytes[bytes.len() - 22..];
        assert_eq!(u16::from_le_bytes([end[10], end[11]]), 1, "the duplicate was written");
    }

    /// The CRC is the one thing in a zip that cannot be checked by looking at
    /// it, so it is checked against the value every CRC-32 implementation
    /// agrees on for this input.
    #[test]
    fn the_crc_is_the_standard_one() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(crc32(b""), 0);
    }

    /// If the system has `unzip`, the archive is opened with it. This is the
    /// only assertion that proves a third party tool agrees.
    #[test]
    fn a_real_unzip_reads_the_archive() {
        let dir = crate::observe::tempdir("bundle-unzip");
        let mut zip = Zip::new();
        zip.add("a.txt", b"hello");
        zip.add("nested/b.txt", b"world");
        let path = dir.join("test.zip");
        std::fs::write(&path, zip.finish()).unwrap();
        // No unzip on this runner means no assertion; the structural test
        // above still holds.
        if let Ok(out) = std::process::Command::new("unzip").arg("-t").arg(&path).output() {
            assert!(
                out.status.success(),
                "unzip refused the archive: {}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_stream_key_never_reaches_the_bundle() {
        let config = r#"
[control]
bind = "0.0.0.0:8080"
token = "hunter2"

[[outputs]]
id = "youtube"
uri = "rtmp://a.rtmp.youtube.com/live2/abcd-efgh-ijkl-mnop"

[[sources]]
id = "cam1"
uri = "rtmp://user:swordfish@camera.local/live/secretkey?auth=zzz"
"#;
        let out = redact_config(config);
        assert!(!out.contains("hunter2"), "{out}");
        assert!(!out.contains("abcd-efgh"), "{out}");
        assert!(!out.contains("swordfish"), "{out}");
        assert!(!out.contains("secretkey"), "{out}");
        assert!(!out.contains("auth=zzz"), "{out}");
        // And what a reader still needs.
        assert!(out.contains("a.rtmp.youtube.com"), "{out}");
        assert!(out.contains("live2"), "{out}");
        assert!(out.contains("0.0.0.0:8080"), "{out}");
    }

    #[test]
    fn a_config_that_does_not_parse_is_left_out_rather_than_shipped() {
        let out = redact_config("token = \"hunter2\"\n= = =");
        assert!(!out.contains("hunter2"), "{out}");
        assert!(out.contains("did not parse"), "{out}");
    }

    #[test]
    fn uri_redaction_keeps_the_shape() {
        assert_eq!(
            redact_uri("rtmp://host/app/key"),
            "rtmp://host/app/REDACTED"
        );
        // Nothing to take out of a two part URL: no key is present.
        assert_eq!(redact_uri("http://example.com/page"), "http://example.com/page");
        assert_eq!(redact_uri("file:///media/clip.mp4"), "file:///media/REDACTED");
        assert_eq!(redact_uri("not a uri"), "not a uri");
    }

    #[tokio::test]
    async fn an_offline_bundle_has_the_versions_the_doctor_and_the_config() {
        gstreamer::init().unwrap();
        let dir = crate::observe::tempdir("bundle-offline");
        let config = dir.join("godwinmix.toml");
        std::fs::write(&config, "[control]\ntoken = \"hunter2\"\n").unwrap();
        std::fs::create_dir_all(dir.join(".godwinmix")).unwrap();
        std::fs::write(dir.join(".godwinmix").join("godwinmix.log"), "{\"level\":\"info\"}\n")
            .unwrap();

        let out = dir.join("bundle.zip");
        let (path, included) = build(&BundleOptions {
            config_path: config,
            runtime_dir: dir.join(".godwinmix"),
            url: None,
            token: None,
            out: out.clone(),
        })
        .await
        .expect("bundle");

        assert_eq!(path, out);
        let joined = included.join(" ");
        assert!(joined.contains("versions.txt"), "{joined}");
        assert!(joined.contains("doctor.txt"), "{joined}");
        assert!(joined.contains("config.redacted.toml"), "{joined}");
        assert!(joined.contains("logs/godwinmix.log"), "{joined}");

        let bytes = std::fs::read(&out).unwrap();
        assert_eq!(&bytes[..4], b"PK\x03\x04");
        assert!(
            !String::from_utf8_lossy(&bytes).contains("hunter2"),
            "the token reached the bundle"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_default_name_is_a_file_name_on_every_platform() {
        let name = default_name();
        assert!(name.ends_with(".zip"));
        assert!(!name.contains(':'), "a colon is not a file name character on Windows: {name}");
        assert!(!name.contains('/'), "{name}");
    }
}
