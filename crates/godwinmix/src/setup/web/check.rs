//! Checking a download against the digest its index publishes.

use std::path::Path;

/// SHA-1 of a file, off the runtime's threads.
pub async fn digest(path: &Path) -> String {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        use sha1::{Digest, Sha1};
        use std::io::Read;
        let Ok(mut f) = std::fs::File::open(&path) else { return String::new() };
        let mut h = Sha1::new();
        let mut buf = vec![0u8; 1 << 20];
        while let Ok(n) = f.read(&mut buf) {
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .await
    .unwrap_or_default()
}
