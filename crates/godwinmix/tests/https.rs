//! HTTPS and HTTP on one control port.
//!
//! A real server on a real port, real GStreamer, a certificate made the way
//! startup makes one. Plain HTTP must keep answering as it always has, HTTPS
//! must answer on the same port with a certificate a client that trusts it
//! accepts for `localhost`, and the WebSocket must upgrade over TLS.

use futures_util::{SinkExt, StreamExt};
use godwinmix::control::{AppState, Engine};
use godwinmix_core::config::Config;
use godwinmix_core::mixer::{self, Mixer};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio_rustls::rustls;
use tokio_tungstenite::tungstenite::Message;

fn app(cfg: &Config) -> AppState {
    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg.clone()).expect("building the mixer");
    mix.start().expect("starting the mixer");
    let (multiview, preview, encoder) = (mix.multiview_handle(), mix.preview_handle(), mix.encoder_handle());
    std::mem::forget(mixer::spawn(mix, cmd_rx, handle.clone()));
    let scenes = godwinmix_core::scene::server::SceneServer::in_memory(godwinmix_core::caps::CanvasCaps::new(&cfg.canvas));
    AppState::new(
        cfg,
        Engine {
            mixer: handle.clone(),
            multiview,
            preview,
            encoder,
            library: Arc::new(godwinmix_core::media::MediaLibrary::new(cfg.media.clone())),
            converter: Arc::new(godwinmix_core::convert::Converter::new(handle.clone(), 1, 5)),
            quit: Arc::new(tokio::sync::Notify::new()),
            scenes,
            plugins: godwinmix_core::plugin::supervisor::Supervisor::detached(),
        },
        false,
    )
}

/// One folder for the whole run, made once: the tests run at the same time
/// and share the process's secret store.
fn scratch() -> std::path::PathBuf {
    static DIR: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("gmx-https-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("GODWINMIX_HOME", dir.join("home"));
        dir
    })
    .clone()
}

/// A served mixer with HTTPS on, and the certificate it answers with.
async fn serve() -> (SocketAddr, String, godwinmix_protocol::TlsInfo) {
    let _ = gstreamer::init();
    let dir = scratch();
    let mut cfg: Config = toml::from_str("").expect("an empty config is every default");
    (cfg.canvas.width, cfg.canvas.height, cfg.canvas.fps) = (320, 180, 15);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address = listener.local_addr().unwrap();
    let config_path = dir.join("godwinmix.toml");
    // One at a time, as one mixer starting would: the second finds the
    // certificate the first made and uses it again.
    static STARTING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = STARTING.lock().unwrap();
    let serving = godwinmix::tls::prepare(&cfg, &address.to_string(), &config_path)
        .expect("a certificate")
        .expect("HTTPS is on by default");
    let cert = std::fs::read_to_string(godwinmix::tls::public_path(&config_path)).expect("the public half on disk");
    let info = serving.info.clone();
    let state = app(&cfg);
    tokio::spawn(async move {
        let _ = godwinmix::control::serve_with(listener, state, Some(serving.acceptor)).await;
    });
    (address, cert, info)
}

fn trusting(cert: &str) -> rustls::ClientConfig {
    use rustls::pki_types::{pem::PemObject, CertificateDer};
    let mut roots = rustls::RootCertStore::empty();
    for c in CertificateDer::pem_slice_iter(cert.as_bytes()) {
        roots.add(c.unwrap()).unwrap();
    }
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_port_answers_http_https_and_a_websocket_over_tls() {
    let (address, cert, info) = serve().await;
    let port = address.port();

    // Plain HTTP, unchanged.
    let plain: Value = reqwest::get(format!("http://127.0.0.1:{port}/api/v1/core/info")).await.unwrap().json().await.unwrap();
    assert_eq!(plain["core"], "godwinmix");

    // HTTPS on the same port, checked against the certificate on disk, for
    // the name localhost. `core.info` says what it is and where.
    let client = reqwest::Client::builder()
        .use_rustls_tls()
        .add_root_certificate(reqwest::Certificate::from_pem(cert.as_bytes()).unwrap())
        .build()
        .unwrap();
    let secure: Value = client.get(format!("https://localhost:{port}/api/v1/core/info")).send().await.unwrap().json().await.unwrap();
    assert_eq!(secure["tls"]["source"], "self_signed");
    assert_eq!(secure["tls"]["fingerprint"], json!(info.fingerprint));
    assert_eq!(secure["tls"]["urls"], json!([format!("https://localhost:{port}/")]), "a loopback bind offers only localhost");

    // The WebSocket upgrade over TLS: a JSON-RPC call on /rpc.
    let connector = tokio_rustls::TlsConnector::from(Arc::new(trusting(&cert)));
    let tcp = tokio::net::TcpStream::connect(address).await.unwrap();
    let tls = connector.connect("localhost".try_into().unwrap(), tcp).await.expect("a TLS handshake that trusts the certificate");
    let (mut socket, _) = tokio_tungstenite::client_async(format!("wss://localhost:{port}/rpc"), tls).await.expect("the upgrade over TLS");
    let call = json!({"jsonrpc": "2.0", "id": 1, "method": "core.info", "params": {}});
    socket.send(Message::Text(call.to_string().into())).await.unwrap();
    let answer = loop {
        let frame = tokio::time::timeout(Duration::from_secs(10), socket.next()).await.expect("an answer").unwrap().unwrap();
        let Message::Text(text) = frame else { continue };
        let value: Value = serde_json::from_str(&text).unwrap();
        if value["id"] == 1 {
            break value;
        }
    };
    assert_eq!(answer["result"]["tls"]["fingerprint"], json!(info.fingerprint));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_client_that_never_speaks_holds_up_nobody_else() {
    let (address, _, _) = serve().await;
    // Connected, silent, and kept open for the rest of the test.
    let _quiet = tokio::net::TcpStream::connect(address).await.unwrap();
    let answer = tokio::time::timeout(
        Duration::from_secs(5),
        reqwest::get(format!("http://{address}/api/v1/core/info")),
    )
    .await
    .expect("answered while another connection sat silent")
    .unwrap();
    assert!(answer.status().is_success());
}
