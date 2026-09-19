//! Stream media to a hidden file, then publish without replacing existing work.

use axum::body::Body;
use futures_util::StreamExt;
use godwinmix_protocol::RpcError;
use std::path::Path;
use tokio::io::AsyncWriteExt;

fn collision(name: &str) -> RpcError {
    RpcError::not_in_state(format!(
        "media file '{name}' already exists or an upload with that name is in progress. \
         Choose a different file name and upload again; the existing file was not changed."
    )).with("name", serde_json::json!(name))
}

pub(super) async fn store(dir: &Path, name: &str, body: Body) -> Result<u64, RpcError> {
    tokio::fs::create_dir_all(dir).await.map_err(|e| RpcError::internal(format!(
        "creating media directory {}: {e}. Choose a writable directory in [media].dir and retry.",
        dir.display()
    )))?;
    let part = dir.join(format!(".{name}.part"));
    let final_path = dir.join(name);
    match tokio::fs::symlink_metadata(&final_path).await {
        Ok(_) => return Err(collision(name)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(RpcError::internal(format!("checking media file: {e}"))),
    }
    // Exclusive creation also serializes two uploads of the same name. Never
    // truncate an unfinished upload or follow a preexisting temporary symlink.
    let mut file = tokio::fs::OpenOptions::new().write(true).create_new(true).open(&part)
        .await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::AlreadyExists { collision(name) }
            else { RpcError::internal(format!("creating upload: {e}")) }
        })?;
    let result = async {
        let mut stream = body.into_data_stream();
        let mut written = 0;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| RpcError::internal(format!("upload interrupted: {e}")))?;
            file.write_all(&chunk).await
                .map_err(|e| RpcError::internal(format!("writing upload: {e}")))?;
            written += chunk.len() as u64;
        }
        file.flush().await.map_err(|e| RpcError::internal(format!("flushing upload: {e}")))?;
        file.sync_all().await.map_err(|e| RpcError::internal(format!("saving upload: {e}")))?;
        Ok::<_, RpcError>(written)
    }.await;
    drop(file);
    let result = match result {
        Ok(written) => {
            // Both paths share a directory and filesystem. Unlike rename,
            // hard_link atomically refuses an existing destination on every
            // supported platform, including a file created during this upload.
            tokio::fs::hard_link(&part, &final_path).await.map(|()| written).map_err(|e| {
                if e.kind() == std::io::ErrorKind::AlreadyExists { collision(name) }
                else { RpcError::internal(format!(
                    "publishing upload without replacing existing media: {e}. \
                     Use a writable media directory on a filesystem that supports hard links."
                )) }
            })
        }
        Err(e) => Err(e),
    };
    let _ = tokio::fs::remove_file(&part).await;
    result
}

#[cfg(test)]
mod tests;
