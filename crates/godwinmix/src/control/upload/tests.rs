use super::*;
use gstreamer as gst;
use gst::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};

struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "gmx-upload-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Directory {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); }
}

fn wav() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&16036u32.to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&8000u32.to_le_bytes());
    bytes.extend_from_slice(&16000u32.to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&16000u32.to_le_bytes());
    bytes.resize(16044, 0);
    bytes
}

#[tokio::test]
async fn real_audio_and_image_uploads_are_listed_and_existing_media_is_preserved() {
    gst::init().unwrap();
    let dir = Directory::new();
    let audio = wav();
    assert_eq!(store(&dir.0, "tone.wav", Body::from(audio.clone())).await.unwrap(), audio.len() as u64);
    assert!(store(&dir.0, "tone.wav", Body::from("replacement")).await.is_err());
    assert_eq!(std::fs::read(dir.0.join("tone.wav")).unwrap(), audio);

    let png = dir.0.join(".generated.png");
    let pipeline = gst::parse::launch(
        "videotestsrc num-buffers=1 ! video/x-raw,width=16,height=16 ! pngenc ! filesink name=file"
    ).unwrap().downcast::<gst::Pipeline>().unwrap();
    pipeline.by_name("file").unwrap().set_property("location", png.to_str().unwrap());
    pipeline.set_state(gst::State::Playing).unwrap();
    let message = pipeline.bus().unwrap().timed_pop_filtered(
        gst::ClockTime::from_seconds(5), &[gst::MessageType::Eos, gst::MessageType::Error]
    ).unwrap();
    pipeline.set_state(gst::State::Null).unwrap();
    assert_eq!(message.type_(), gst::MessageType::Eos);
    let image = std::fs::read(png).unwrap();
    store(&dir.0, "still.png", Body::from(image.clone())).await.unwrap();
    assert_eq!(std::fs::read(dir.0.join("still.png")).unwrap(), image);
    let lib = godwinmix_core::media::MediaLibrary::new(godwinmix_core::config::MediaConfig {
        dir: dir.0.display().to_string(), ..Default::default()
    });
    let listing = tokio::task::spawn_blocking(move || lib.list_with(None)).await.unwrap();
    assert!(listing.error.is_none());
    assert_eq!(listing.items.len(), 2);
    assert!(listing.items.iter().any(|item| item.name == "tone.wav" && item.has_audio && !item.has_video));
    assert!(listing.items.iter().any(|item| item.name == "still.png" && item.has_video));
}

#[tokio::test]
async fn concurrent_upload_and_late_destination_collision_preserve_both_files() {
    let dir = Directory::new();
    let path = dir.0.clone();
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, wait) = tokio::sync::oneshot::channel();
    let body = Body::from_stream(futures_util::stream::once(async move {
        started.send(()).unwrap();
        wait.await.unwrap();
        Ok::<_, std::io::Error>(wav())
    }));
    let pending = tokio::spawn(async move { store(&path, "tone.wav", body).await });
    ready.await.unwrap();
    let error = store(&dir.0, "tone.wav", Body::from("another upload")).await.unwrap_err();
    assert!(error.message.contains("Choose a different file name"));
    assert!(dir.0.join(".tone.wav.part").exists());
    std::fs::write(dir.0.join("tone.wav"), b"existing recording").unwrap();
    release.send(()).unwrap();
    assert!(pending.await.unwrap().is_err());
    assert_eq!(std::fs::read(dir.0.join("tone.wav")).unwrap(), b"existing recording");
    assert!(!dir.0.join(".tone.wav.part").exists());
}

#[tokio::test]
async fn interrupted_upload_never_publishes_a_partial_file() {
    let dir = Directory::new();
    let body = Body::from_stream(futures_util::stream::iter([
        Ok(wav()), Err(std::io::Error::other("connection closed"))
    ]));
    assert!(store(&dir.0, "tone.wav", body).await.is_err());
    assert_eq!(std::fs::read_dir(&dir.0).unwrap().count(), 0);
}
