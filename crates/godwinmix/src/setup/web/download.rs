//! Downloads that survive a bad connection.
//!
//! Every request is tried again after a pause that grows, up to a few times
//! in a row with nothing gained. A file is written to a `.part` beside where
//! it goes and asked for again from the byte it reached, so a connection that
//! drops at 90 MB costs the last few seconds and not the whole download. A
//! server that will not resume (a 200 to a range request) starts it over.

use std::path::Path;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

/// Attempts in a row that get nothing before giving up.
const ATTEMPTS: u32 = 6;
/// The longest a read may wait for the next bytes before the attempt is
/// treated as dropped.
const STALL: Duration = Duration::from_secs(60);

pub fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .user_agent(concat!("godwinmix/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("making an HTTP client: {e}"))
}

async fn pause(failures: u32) {
    let secs = (2u64 << failures.min(4)).min(30);
    tokio::time::sleep(Duration::from_secs(secs)).await;
}

/// A JSON document, tried again until it comes whole.
pub async fn json(client: &reqwest::Client, url: &str) -> Result<serde_json::Value, String> {
    let mut last = String::new();
    for failures in 0..ATTEMPTS {
        let got = async {
            let resp = client.get(url).send().await.and_then(|r| r.error_for_status()).map_err(|e| e.to_string())?;
            tokio::time::timeout(Duration::from_secs(300), resp.json::<serde_json::Value>())
                .await
                .map_err(|_| "timed out reading".to_string())?
                .map_err(|e| e.to_string())
        }
        .await;
        match got {
            Ok(v) => return Ok(v),
            Err(e) => {
                tracing::warn!(%url, attempt = failures + 1, error = %e, "a download failed; trying again");
                last = e;
                pause(failures).await;
            }
        }
    }
    Err(format!("{url}: {last}"))
}

/// Download `url` into `part` until it holds `size` bytes, carrying on from
/// whatever is there. `on` hears the byte count as it grows.
pub async fn resume(
    client: &reqwest::Client,
    url: &str,
    part: &Path,
    size: u64,
    on: impl Fn(u64),
) -> Result<(), String> {
    let mut failures = 0;
    let mut last = String::new();
    while failures < ATTEMPTS {
        let have = std::fs::metadata(part).map(|m| m.len()).unwrap_or(0);
        if have == size {
            return Ok(());
        }
        let from = if have > size { 0 } else { have };
        match attempt(client, url, part, from, &on).await {
            Ok(gained) if gained > 0 => failures = 0,
            Ok(_) => {
                failures += 1;
                last = "the server sent nothing".into();
                pause(failures).await;
            }
            Err((gained, e)) => {
                tracing::warn!(%url, have = from + gained, error = %e, "the download dropped; carrying on from there");
                last = e;
                failures = if gained > 0 { 0 } else { failures + 1 };
                pause(failures).await;
            }
        }
    }
    Err(format!("{url}: {last}"))
}

/// One request from byte `from`. Answers what it gained, or how far it got
/// and why it stopped.
async fn attempt(
    client: &reqwest::Client,
    url: &str,
    part: &Path,
    from: u64,
    on: &impl Fn(u64),
) -> Result<u64, (u64, String)> {
    let mut req = client.get(url);
    if from > 0 {
        req = req.header(reqwest::header::RANGE, format!("bytes={from}-"));
    }
    let resp = req.send().await.and_then(|r| r.error_for_status()).map_err(|e| (0, e.to_string()))?;
    // 206 carries on; anything else is the whole file again.
    let resumed = resp.status() == reqwest::StatusCode::PARTIAL_CONTENT;
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(resumed)
        .truncate(!resumed)
        .open(part)
        .await
        .map_err(|e| (0, format!("opening {}: {e}", part.display())))?;
    let start = if resumed { from } else { 0 };
    let mut resp = resp;
    let mut gained = 0u64;
    loop {
        let chunk = match tokio::time::timeout(STALL, resp.chunk()).await {
            Err(_) => return Err((gained, "no data for a minute".into())),
            Ok(Err(e)) => return Err((gained, e.to_string())),
            Ok(Ok(None)) => break,
            Ok(Ok(Some(c))) => c,
        };
        file.write_all(&chunk).await.map_err(|e| (gained, format!("writing {}: {e}", part.display())))?;
        gained += chunk.len() as u64;
        on(start + gained);
    }
    file.flush().await.map_err(|e| (gained, e.to_string()))?;
    Ok(gained)
}
