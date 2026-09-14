//! Installing from a release, and rolling an update back, against a real
//! server.
//!
//! The network here is real: a TCP listener on loopback serving the two
//! documents a GitHub release install actually reads, a release JSON and the
//! asset bytes. Nothing is stubbed inside the code under test, so what these
//! tests exercise is the path an operator takes, down to the tar being
//! unpacked and the executable bit surviving it.
//!
//! The plugin is a shell script for the same reason `sidecar.rs` uses one: if
//! the protocol can be implemented in forty lines of `sh`, the promise that a
//! plugin author needs no SDK holds. It needs `gst-launch-1.0` for the media
//! checks and says so and skips rather than failing where that is missing.

// Unix only, and the whole file rather than each test: the plugin these drive
// is a shell script. Windows compiles and runs every ungated test in the
// workspace on its own CI runner, and the platform arms this file would
// exercise are listed in docs/explanation/cross-platform.md.
#![cfg(unix)]

use godwinmix_core::plugin::loader;
use godwinmix_host::verify::sha256;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// The plugin under test
// ---------------------------------------------------------------------------

/// A working source: handshake, media, health, shutdown.
const WORKING: &str = r#"#!/bin/sh
say() { printf '%s\n' "$1" >&2; }
say '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"plugin":"relbars","version":"VERSION","api":1,"transports":["container"],"provides":[]}}'
read -r _ready
say '{"jsonrpc":"2.0","method":"initialized"}'
media_started=0
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"method":"start"'*)
      if [ "$media_started" = 0 ]; then
        gst-launch-1.0 -q \
          videotestsrc is-live=true pattern=smpte \
          ! video/x-raw,format=I420,width=640,height=360,framerate=30/1 \
          ! matroskamux streamable=true name=mux \
          ! fdsink fd=1 &
        media_started=1
      fi
      say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"latency_ms\":0}}"
      ;;
    *'"method":"health"'*) say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"state\":\"ok\"}}" ;;
    *'"method":"configure"'*) say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"applied\":true}}" ;;
    *'"method":"stop"'*) say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{}}" ;;
    *'"method":"shutdown"'*) exit 0 ;;
  esac
done
"#;

/// The same plugin, one release later, with a mistake in it: it dies before it
/// says anything. This is what an update has to survive.
const BROKEN: &str = r#"#!/bin/sh
echo 'Traceback: no module named gmx' >&2
exit 1
"#;

fn manifest(version: &str) -> String {
    format!(
        r#"[plugin]
name = "relbars"
version = "{version}"
api = 1
description = "Colour bars, delivered as a signed release asset."
license = "MIT"
platforms = ["{platform}"]
placements = ["sidecar"]
process = "per-instance"

[run]
shell = "run.sh"

[[provides]]
kind = "source"
id = "source"
media = {{ video = "raw", audio = "none", alpha = false, thumb = true }}
transports = ["container"]
capabilities = ["restart-in-place", "health"]
latency_ms = 0
settings = "settings.json"
"#,
        platform = godwinmix_host::launch::this_platform()
    )
}

/// Write a plugin directory, ready to be packed.
fn write_plugin(at: &Path, version: &str, body: &str) {
    std::fs::create_dir_all(at).expect("the plugin directory");
    std::fs::write(at.join("gmx-plugin.toml"), manifest(version)).expect("the manifest");
    // A source provide must declare a settings schema, and the manifest check
    // says so. An empty object is a schema: this plugin takes no settings.
    std::fs::write(
        at.join("settings.json"),
        "{\n  \"$schema\": \"https://json-schema.org/draft/2020-12/schema\",\n  \"title\": \"Colour bars\",\n  \"type\": \"object\",\n  \"properties\": {},\n  \"additionalProperties\": false\n}\n",
    )
    .expect("the settings schema");
    let entry = at.join("run.sh");
    std::fs::write(&entry, body.replace("VERSION", version)).expect("the entry point");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&entry, std::fs::Permissions::from_mode(0o755))
        .expect("the executable bit");
}

/// `tar czf` the directory, the way the index CI packages an asset.
fn pack(dir: &Path, into: &Path) -> Vec<u8> {
    let parent = dir.parent().expect("the plugin has a parent directory");
    let name = dir.file_name().expect("the plugin has a name");
    let status = std::process::Command::new("tar")
        .arg("-czf")
        .arg(into)
        .arg("-C")
        .arg(parent)
        .arg(name)
        .status()
        .expect("tar runs");
    assert!(status.success(), "tar packed the plugin");
    std::fs::read(into).expect("the asset")
}

// ---------------------------------------------------------------------------
// A signature
// ---------------------------------------------------------------------------

/// A sigstore bundle covering `bytes`, in the current format.
///
/// Only the digest is real, which is exactly the level of check this test is
/// about: with no cosign on the machine the verifier proves the bytes that
/// arrived are the bytes the bundle names, and says so in the label rather
/// than claiming more.
fn bundle_for(bytes: &[u8]) -> String {
    serde_json::json!({
        "mediaType": "application/vnd.dev.sigstore.bundle.v0.3+json",
        "verificationMaterial": {
            "certificate": { "rawBytes": "MIIBpretend" },
            "tlogEntries": [{ "logIndex": "412200", "integratedTime": "1757800000" }]
        },
        "messageSignature": {
            "messageDigest": { "algorithm": "SHA2_256", "digest": base64(&sha256::digest(bytes)) },
            "signature": "MEUCIQpretend"
        }
    })
    .to_string()
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[((n >> (18 - i * 6)) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The server
// ---------------------------------------------------------------------------

/// A few dozen lines of HTTP, so the test needs no server crate and no
/// network. It answers exactly what a release install asks for and 404s
/// everything else, which is also how a missing signature file is tested.
///
/// The routes are added after it is bound, because a release JSON has to carry
/// the address of the server serving it and that address is not known until
/// the listener exists.
struct Server {
    base: String,
    routes: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl Server {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a loopback port");
        let base = format!("http://{}", listener.local_addr().expect("the address"));
        let routes: Arc<Mutex<HashMap<String, Vec<u8>>>> = Arc::new(Mutex::new(HashMap::new()));
        let served = routes.clone();
        std::thread::Builder::new()
            .name("fake-release-server".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let routes = served.clone();
                    // One thread per connection: reqwest keeps a connection
                    // alive between the release JSON and the asset, and a
                    // single threaded loop would serialise them into a stall.
                    std::thread::spawn(move || serve(stream, &routes));
                }
            })
            .expect("the server thread");
        Self { base, routes }
    }

    fn route(&self, path: impl Into<String>, body: Vec<u8>) {
        self.routes.lock().expect("the routes").insert(path.into(), body);
    }

    fn serve_release(&self, repo: &str, tag: &str, release: &Release) {
        for (path, body) in &release.routes {
            self.route(path.clone(), body.clone());
        }
        let mut names = vec![release.asset.clone()];
        if release.signed {
            names.push(format!("{}.sigstore.json", release.asset));
        }
        let listed: Vec<&str> = names.iter().map(String::as_str).collect();
        self.route(
            format!("/repos/{repo}/releases/latest"),
            release_json(&self.base, tag, &listed),
        );
        self.route(
            format!("/repos/{repo}/releases/tags/{tag}"),
            release_json(&self.base, tag, &listed),
        );
    }
}

fn serve(mut stream: std::net::TcpStream, routes: &Mutex<HashMap<String, Vec<u8>>>) {
    loop {
        let mut reader = BufReader::new(match stream.try_clone() {
            Ok(s) => s,
            Err(_) => return,
        });
        let mut request = String::new();
        if reader.read_line(&mut request).unwrap_or(0) == 0 {
            return;
        }
        // Drain the headers; nothing here reads a body.
        loop {
            let mut header = String::new();
            match reader.read_line(&mut header) {
                Ok(0) => return,
                Ok(_) if header.trim().is_empty() => break,
                Ok(_) => {}
                Err(_) => return,
            }
        }
        let path = request.split_whitespace().nth(1).unwrap_or("/").to_string();
        let found = routes.lock().expect("the routes").get(&path).cloned();
        let answer = match found {
            Some(body) => {
                let mut head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: keep-alive\r\n\r\n",
                    body.len()
                )
                .into_bytes();
                head.extend_from_slice(&body);
                head
            }
            None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\nConnection: keep-alive\r\n\r\nnot found".to_vec(),
        };
        if stream.write_all(&answer).is_err() || stream.flush().is_err() {
            return;
        }
    }
}

/// The release JSON GitHub answers with, for the assets given.
fn release_json(base: &str, tag: &str, assets: &[&str]) -> Vec<u8> {
    let listed: Vec<serde_json::Value> = assets
        .iter()
        .map(|name| {
            serde_json::json!({
                "name": name,
                "browser_download_url": format!("{base}/dl/{name}")
            })
        })
        .collect();
    serde_json::json!({ "tag_name": tag, "assets": listed })
        .to_string()
        .into_bytes()
}

// ---------------------------------------------------------------------------
// Scaffolding
// ---------------------------------------------------------------------------

fn temp(what: &str) -> PathBuf {
    let dir = std::env::temp_dir()
        .join(format!("gmx-ecosystem-{}-{what}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

/// One lock for the file: the plugin registry is one per process.
fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn which(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|d| d.join(name).is_file()))
        .unwrap_or(false)
}

/// The asset name the index CI writes, and the one `asset_for` looks for.
fn asset_name(version: &str) -> String {
    format!("relbars-{version}-{}.tar.gz", godwinmix_host::launch::this_platform())
}

/// A release of the plugin, packed and signed, ready to serve.
struct Release {
    routes: HashMap<String, Vec<u8>>,
    asset: String,
    signed: bool,
}

fn build_release(root: &Path, version: &str, body: &str, sign: bool) -> Release {
    let source = root.join(format!("build-{version}")).join("relbars");
    write_plugin(&source, version, body);
    let asset = asset_name(version);
    let bytes = pack(&source, &root.join(&asset));
    let mut routes = HashMap::new();
    routes.insert(format!("/dl/{asset}"), bytes.clone());
    if sign {
        routes.insert(
            format!("/dl/{asset}.sigstore.json"),
            bundle_for(&bytes).into_bytes(),
        );
    }
    Release { routes, asset, signed: sign }
}

/// Point the loader at a fresh plugins directory and the source code at the
/// fake server. Both are process wide, which is what `exclusive` is for.
fn point_at(server: &Server, plugins: &Path) {
    std::fs::create_dir_all(plugins).expect("the plugins directory");
    loader::set_dir(plugins.to_path_buf());
    std::env::set_var("GMX_GITHUB_API", &server.base);
    // The machine running the tests may well have cosign installed, and then
    // it would refuse a bundle whose signature is not real. The two level
    // check is the thing under test here; `verify/tests.rs` covers the words
    // cosign's own refusal produces.
    std::env::set_var("GMX_NO_COSIGN", "1");
}

fn options() -> loader::InstallOptions {
    loader::InstallOptions::default()
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

#[test]
fn a_signed_release_installs_and_the_source_it_provides_works() {
    let _lock = exclusive();
    let root = temp("release");
    let server = Server::start();
    let release = build_release(&root, "1.0.0", WORKING, true);
    server.serve_release("psmux/gmx-relbars", "v1.0.0", &release);

    let plugins = root.join("plugins");
    point_at(&server, &plugins);

    let installed = loader::install("psmux/gmx-relbars", &options()).expect("it installs");
    assert_eq!(installed.name(), "relbars");
    assert_eq!(installed.version(), "1.0.0");
    assert_eq!(
        installed.trust.label(),
        "signed, digest only",
        "with no cosign on the machine the bundle's digest is what was checked, and the \
         label says exactly that"
    );
    assert_eq!(
        installed.trust.source, "psmux/gmx-relbars",
        "an unpinned add records the unpinned source, or `gmx plugin update` with no \
         source would refetch the release it already has"
    );
    assert_eq!(installed.trust.resolved, "v1.0.0");
    assert!(
        installed.provides.contains(&"relbars/source".to_string()),
        "the provide is registered: {:?}",
        installed.provides
    );

    // The executable bit survived the tar and the copy, or nothing would run.
    use std::os::unix::fs::PermissionsExt;
    let entry = plugins.join("relbars").join("1.0.0").join("run.sh");
    assert_eq!(
        std::fs::metadata(&entry).expect("the entry point").permissions().mode() & 0o100,
        0o100
    );

    // The trust record is on disk, so a core that restarts still knows.
    let reread = loader::read(&installed.root, &Default::default());
    assert_eq!(reread.trust.label(), "signed, digest only");

    if !which("gst-launch-1.0") {
        println!("skipping the harness: gst-launch-1.0 is not on PATH");
        return;
    }
    let _ = gstreamer::init();
    let report = godwinmix_core::plugin::harness::check_plugin(&installed.root, true)
        .expect("the harness runs");
    for line in report.lines() {
        println!("  {line}");
    }
    report.into_result().expect("a plugin installed from a release is usable");
}

#[test]
fn an_unsigned_release_is_refused_when_the_operator_asked_for_signatures() {
    let _lock = exclusive();
    let root = temp("unsigned");
    let server = Server::start();
    let release = build_release(&root, "1.0.0", WORKING, false);
    server.serve_release("psmux/gmx-relbars", "v1.0.0", &release);
    point_at(&server, &root.join("plugins"));

    let strict = loader::InstallOptions { allow_unsigned: false, ..Default::default() };
    let err = loader::install("psmux/gmx-relbars", &strict).expect_err("nothing signed it");
    let text = format!("{err:#}");
    assert!(text.contains("allow_unsigned"), "{text}");
    assert!(text.contains("sigstore.json"), "{text}");

    // The same release installs when the operator has not asked for more.
    let installed = loader::install("psmux/gmx-relbars", &options()).expect("it installs");
    assert_eq!(installed.trust.label(), "custom, unreviewed");
}

#[test]
fn an_asset_whose_bytes_changed_after_signing_never_reaches_the_plugins_directory() {
    let _lock = exclusive();
    let root = temp("tampered");
    let server = Server::start();
    let release = build_release(&root, "1.0.0", WORKING, true);
    server.serve_release("psmux/gmx-relbars", "v1.0.0", &release);
    // Somebody swapped the asset after the bundle was made.
    let mut swapped = release.routes[&format!("/dl/{}", release.asset)].clone();
    swapped.extend_from_slice(b"and one more byte");
    server.route(format!("/dl/{}", release.asset), swapped);

    let plugins = root.join("plugins");
    point_at(&server, &plugins);
    let err = loader::install("psmux/gmx-relbars", &options()).expect_err("the digest differs");
    assert!(format!("{err:#}").contains("truncated"), "{err:#}");
    assert!(
        !plugins.join("relbars").exists(),
        "nothing is copied until the signature has had its say"
    );
}

#[test]
fn a_release_with_no_asset_for_this_platform_lists_the_platforms_it_has() {
    let _lock = exclusive();
    let root = temp("platform");
    let server = Server::start();
    // A release built for a machine that is not this one.
    let other = if godwinmix_host::launch::this_platform() == "linux-x86_64" {
        "windows-x86_64"
    } else {
        "linux-x86_64"
    };
    let asset = format!("relbars-1.0.0-{other}.tar.gz");
    server.route(format!("/dl/{asset}"), b"not unpacked, the refusal comes first".to_vec());
    server.route(
        "/repos/psmux/gmx-relbars/releases/latest",
        release_json(&server.base, "v1.0.0", &[&asset]),
    );
    point_at(&server, &root.join("plugins"));

    let err = loader::install("psmux/gmx-relbars", &options()).expect_err("wrong platform");
    let text = format!("{err:#}");
    assert!(text.contains(other), "the refusal names what the release does have: {text}");
    assert!(text.contains(godwinmix_host::launch::this_platform()), "{text}");
    assert!(text.contains(".git"), "and the way to build it here: {text}");
}

#[test]
fn an_update_whose_new_build_never_says_hello_is_rolled_back() {
    let _lock = exclusive();
    let root = temp("rollback");
    let server = Server::start();
    let plugins = root.join("plugins");
    point_at(&server, &plugins);

    // 1.0.0 is installed and works.
    let good = build_release(&root, "1.0.0", WORKING, true);
    server.serve_release("psmux/gmx-relbars", "v1.0.0", &good);
    let first = loader::install("psmux/gmx-relbars", &options()).expect("it installs");
    assert_eq!(first.version(), "1.0.0");

    // 1.1.0 is published, and it exits on startup.
    let bad = build_release(&root, "1.1.0", BROKEN, true);
    server.serve_release("psmux/gmx-relbars", "v1.1.0", &bad);
    let err = loader::update("relbars", "psmux/gmx-relbars@1.1.0", &options())
        .expect_err("1.1.0 never says hello");
    let text = format!("{err:#}");
    assert!(text.contains("still running"), "{text}");
    assert!(text.contains("1.0.0"), "the version that is still working is named: {text}");
    assert!(
        text.contains("Traceback"),
        "what the plugin printed before dying is in the message: {text}"
    );

    // The working version is where it was, in the registry and on the disk.
    let now = loader::get("relbars").expect("it is still installed");
    assert_eq!(now.version(), "1.0.0");
    assert!(now.problem.is_none(), "{:?}", now.problem);
    assert!(plugins.join("relbars").join("1.0.0").join("run.sh").is_file());
    assert!(
        !plugins.join("relbars").join("1.1.0").exists(),
        "the build that failed is not left behind"
    );
    // And no rollback directory is left for the next scan to read as a version.
    let versions: Vec<String> = std::fs::read_dir(plugins.join("relbars"))
        .expect("the plugin directory")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(versions, vec!["1.0.0".to_string()], "{versions:?}");
}

#[test]
fn an_update_to_a_build_that_starts_replaces_the_old_one() {
    let _lock = exclusive();
    let root = temp("update-ok");
    let server = Server::start();
    let plugins = root.join("plugins");
    point_at(&server, &plugins);

    let first = build_release(&root, "1.0.0", WORKING, true);
    server.serve_release("psmux/gmx-relbars", "v1.0.0", &first);
    loader::install("psmux/gmx-relbars", &options()).expect("it installs");

    let next = build_release(&root, "1.2.0", WORKING, true);
    server.serve_release("psmux/gmx-relbars", "v1.2.0", &next);
    // No source given: it goes back to where it came from, which must be the
    // unpinned `psmux/gmx-relbars` and not the v1.0.0 it resolved to.
    let asked = loader::get("relbars").expect("installed").trust.source.clone();
    assert_eq!(asked, "psmux/gmx-relbars");
    let updated = loader::update("relbars", &asked, &options()).expect("1.2.0 says hello");
    assert_eq!(updated.from, "1.0.0");
    assert_eq!(updated.to, "1.2.0");
    assert_eq!(loader::get("relbars").expect("installed").version(), "1.2.0");
    assert!(
        !plugins.join("relbars").join("1.0.0").exists(),
        "the version it replaced is gone, not left to be scanned as a second version"
    );
}

#[test]
fn a_plugin_ahead_of_this_core_says_which_core_would_run_it() {
    let _lock = exclusive();
    let root = temp("api");
    let server = Server::start();
    let source = root.join("ahead").join("relbars");
    write_plugin(&source, "2.0.0", WORKING);
    // Rewrite the manifest to claim an api level nobody has released.
    let text = std::fs::read_to_string(source.join("gmx-plugin.toml")).expect("the manifest");
    std::fs::write(source.join("gmx-plugin.toml"), text.replace("api = 1", "api = 9"))
        .expect("the manifest");
    point_at(&server, &root.join("plugins"));

    let err = loader::install(&source.display().to_string(), &options())
        .expect_err("api 9 does not exist");
    let text = format!("{err:#}");
    assert!(text.contains("api 9"), "{text}");
    assert!(text.contains("no released core speaks api 9"), "{text}");
}

#[test]
fn a_local_directory_still_installs_exactly_as_it_did() {
    let _lock = exclusive();
    let root = temp("path");
    let source = root.join("checkout").join("relbars");
    write_plugin(&source, "0.1.0", WORKING);
    let plugins = root.join("plugins");
    std::fs::create_dir_all(&plugins).expect("the plugins directory");
    loader::set_dir(plugins.clone());

    let installed = loader::install_from_path(&source).expect("it installs");
    assert_eq!(installed.name(), "relbars");
    assert_eq!(installed.trust.label(), "custom, unreviewed");
    assert!(installed.trust.explanation().contains("permissions you give it"));
    assert!(plugins.join("relbars").join("0.1.0").join("run.sh").is_file());
}
