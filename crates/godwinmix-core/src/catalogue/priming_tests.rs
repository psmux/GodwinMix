//! Every AAC encoder's `priming_delay_ms`, measured.
//!
//! The mixer holds the programme's video back by the chosen AAC encoder's
//! priming delay, so the number in `codecs.toml` is what keeps lips on the
//! sound. Here a beep at exactly 500 ms goes through each installed encoder
//! and each installed AAC decoder, real elements both, and the time the beep
//! comes out at, read from the decoded buffers' timestamps, is compared with
//! the catalogue. A new encoder, or a GStreamer that starts trimming its own
//! delay, fails here rather than on air. Whatever this platform does not have
//! is skipped and said.

use super::libav::ladders;
use super::model::Role;
use super::select::GstRegistry;
use super::Catalogue;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

const RATE: usize = 48_000;

/// What a decoder takes off the front of the sound itself, in milliseconds.
/// `priming_delay_ms` is the delay a viewer's player shows, and players
/// decode as FFmpeg does, leaving the encoder's delay in. faad drops the
/// first 1024 samples of every stream, measured on Linux: `voaacenc` came
/// out 12.1 ms late through it and 33.4 ms through `avdec_aac` and
/// `mfaacdec`. So a figure read through faad has that added back.
fn decoder_trim_ms(decoder: &str) -> f64 {
    match decoder {
        "faad" => 1024.0 * 1000.0 / RATE as f64,
        _ => 0.0,
    }
}
const BEEP_AT: usize = RATE / 2;

/// Two seconds of stereo silence with 100 ms of a 1 kHz tone at 500 ms.
fn beep() -> gst::Buffer {
    let mut samples = Vec::with_capacity(RATE * 2 * 2 * 2);
    for i in 0..RATE * 2 {
        let v = if (BEEP_AT..BEEP_AT + RATE / 10).contains(&i) {
            (16_000.0 * (2.0 * std::f64::consts::PI * 1000.0 * (i - BEEP_AT) as f64 / RATE as f64).sin()) as i16
        } else {
            0
        };
        for _ in 0..2 {
            samples.extend_from_slice(&v.to_le_bytes());
        }
    }
    let mut buf = gst::Buffer::from_mut_slice(samples);
    let b = buf.get_mut().unwrap();
    b.set_pts(gst::ClockTime::ZERO);
    b.set_duration(gst::ClockTime::from_seconds(2));
    buf
}

/// Where the beep comes out of `encoder` and `decoder`, in milliseconds after
/// it went in.
fn measure(encoder: &str, decoder: &str) -> Option<f64> {
    let line = format!(
        "appsrc name=src format=time caps=audio/x-raw,format=S16LE,layout=interleaved,rate=48000,channels=2 \
         ! audioconvert ! {encoder} name=enc ! aacparse ! {decoder} ! audioconvert \
         ! audio/x-raw,format=F32LE,layout=interleaved,channels=2 ! appsink name=sink sync=false"
    );
    let pipeline = gst::parse::launch(&line).ok()?.downcast::<gst::Pipeline>().ok()?;
    crate::probe::configure_audio_encoder(&pipeline.by_name("enc")?, 128);
    let src = pipeline.by_name("src")?.downcast::<gst_app::AppSrc>().ok()?;
    let sink = pipeline.by_name("sink")?.downcast::<gst_app::AppSink>().ok()?;
    pipeline.set_state(gst::State::Playing).ok()?;
    src.push_buffer(beep()).ok()?;
    src.end_of_stream().ok()?;
    let mut onset = None;
    while let Ok(sample) = sink.pull_sample() {
        let (Some(buf), Some(caps)) = (sample.buffer(), sample.caps()) else { continue };
        let rate = caps.structure(0)?.get::<i32>("rate").ok()? as f64;
        let pts = buf.pts()?.nseconds() as f64 / 1e9;
        let map = buf.map_readable().ok()?;
        let frames = map.chunks_exact(8).map(|f| f32::from_le_bytes([f[0], f[1], f[2], f[3]]));
        if let Some(i) = frames.into_iter().position(|v| v.abs() > 0.25) {
            onset = Some((pts + i as f64 / rate) * 1000.0 - 500.0);
            break;
        }
    }
    let _ = pipeline.set_state(gst::State::Null);
    onset
}

#[test]
fn every_installed_aac_encoder_s_priming_delay_matches_the_catalogue() {
    if gst::init().is_err() {
        return;
    }
    let cat = Catalogue::shipped().unwrap();
    let decoders = ladders(&cat, Role::Decode, &GstRegistry).remove("aac").unwrap_or_default();
    let mut wrong = Vec::new();
    let mut measured = 0;
    for e in cat.audio.iter().filter(|e| e.codec == "aac" && !e.disabled) {
        let Some(encoder) = e.encoder.as_deref() else { continue };
        if !crate::probe::exists(encoder) {
            println!("skipping {encoder}: not installed here");
            continue;
        }
        let want = e.priming_delay_ms.unwrap_or(0) as f64;
        for (decoder, _) in &decoders {
            let Some(got) = measure(encoder, decoder) else {
                wrong.push(format!("{encoder} through {decoder}: the beep never came out"));
                continue;
            };
            measured += 1;
            let got = got + decoder_trim_ms(decoder);
            println!("{encoder} through {decoder}: beep {got:.1} ms late as a player sees it, catalogue says {want} ms");
            if (got - want).abs() > 4.0 {
                wrong.push(format!("{encoder} through {decoder}: {got:.1} ms, catalogue says {want} ms"));
            }
        }
    }
    if decoders.is_empty() {
        println!("skipping: no AAC decoder installed");
    }
    assert!(wrong.is_empty(), "priming_delay_ms in codecs.toml is wrong:\n  {}", wrong.join("\n  "));
    println!("{measured} encoder and decoder pairs measured");
}
