//! The built in presets, and turning what an output asked for into the
//! requests the planner takes. Data, so the page, the core and a channel
//! destination all read one table.

use godwinmix_protocol::rendition::{
    AudioCodec, AudioWant, Container, Fps, RenditionChoice, RenditionPreset, RenditionRequest,
    VideoCodec, VideoWant,
};

/// The keyframe interval every platform preset asks for.
const GOP_MS: u32 = 2000;

fn aac(kbps: u32) -> Option<AudioWant> {
    Some(AudioWant {
        codec: Some(AudioCodec::Aac),
        channels: Some(2),
        sample_rate: Some(48_000),
        bitrate_kbps: Some(kbps),
    })
}

fn h264(width: u32, height: u32, fps: Option<u32>, kbps: u32) -> Option<VideoWant> {
    Some(VideoWant {
        codec: Some(VideoCodec::H264),
        width: Some(width),
        height: Some(height),
        fps: fps.map(Fps::whole),
        bitrate_kbps: Some(kbps),
        keyframe_ms: Some(GOP_MS),
        ..VideoWant::default()
    })
}

fn request(id: &str, container: Container, video: Option<VideoWant>) -> RenditionRequest {
    RenditionRequest {
        id: id.into(),
        container,
        video,
        audio: aac(128),
        ..RenditionRequest::default()
    }
}

fn single(id: &str, title: &str, r: RenditionRequest) -> RenditionPreset {
    RenditionPreset {
        id: id.into(),
        title: title.into(),
        group: "platform".into(),
        request: r,
        ladder: None,
        cost: None,
        available: true,
        why: None,
    }
}

fn ladder(id: &str, title: &str, rungs: &[(u32, u32, u32)]) -> RenditionPreset {
    let rungs: Vec<RenditionRequest> = rungs
        .iter()
        .map(|&(w, h, kbps)| request(&format!("{h}p"), Container::Hls, h264(w, h, None, kbps)))
        .collect();
    RenditionPreset {
        id: id.into(),
        title: title.into(),
        group: "ladder".into(),
        request: rungs[0].clone(),
        ladder: Some(rungs),
        cost: None,
        available: true,
        why: None,
    }
}

/// Every preset this build knows, before asking what this machine can make.
pub fn builtin() -> Vec<RenditionPreset> {
    let flv = Container::Flv;
    let audio_only = RenditionRequest {
        no_video: true,
        ..request("audio-only-aac", flv, None)
    };
    let copy = RenditionRequest {
        id: "copy".into(),
        ..RenditionRequest::default()
    };
    vec![
        single("youtube-1080p30", "YouTube 1080p30", request("youtube-1080p30", flv, h264(1920, 1080, Some(30), 6000))),
        single("youtube-720p30", "YouTube 720p30", request("youtube-720p30", flv, h264(1280, 720, Some(30), 3000))),
        single("facebook-720p30", "Facebook 720p30", request("facebook-720p30", flv, h264(1280, 720, Some(30), 4000))),
        single("twitch-1080p60", "Twitch 1080p60", request("twitch-1080p60", flv, h264(1920, 1080, Some(60), 6000))),
        single("twitch-720p30", "Twitch 720p30", request("twitch-720p30", flv, h264(1280, 720, Some(30), 3000))),
        RenditionPreset {
            group: "audio".into(),
            ..single("audio-only-aac", "Sound only, AAC", audio_only)
        },
        ladder("abr-ladder-4", "Adaptive, 4 steps", &[(1920, 1080, 5000), (1280, 720, 2800), (854, 480, 1400), (640, 360, 800)]),
        ladder("abr-ladder-3", "Adaptive, 3 steps", &[(1280, 720, 2800), (854, 480, 1400), (640, 360, 800)]),
        RenditionPreset {
            group: "copy".into(),
            ..single("copy", "As it comes (no conversion)", copy)
        },
    ]
}

/// The preset with this id.
pub fn preset(id: &str) -> Option<RenditionPreset> {
    builtin().into_iter().find(|p| p.id == id)
}

/// The id a preset or request is known by in the page: the preset id, or
/// `custom`.
pub fn choice_label(choice: &RenditionChoice) -> String {
    match choice {
        RenditionChoice::Preset(p) => p.preset.clone(),
        RenditionChoice::Request(_) | RenditionChoice::Ladder(_) => "custom".into(),
    }
}

/// The requests one output asks for, with ids the planner can tell apart:
/// the output's own id for one rendition, `<output>-<rung>` for each rung of
/// a ladder. `Ok(None)` is the `copy` preset: no rendition at all.
pub fn expand(
    output: &str,
    choice: &RenditionChoice,
) -> Result<Option<Vec<RenditionRequest>>, String> {
    let rungs = match choice {
        RenditionChoice::Request(r) => vec![RenditionRequest {
            id: output.into(),
            ..r.clone()
        }],
        RenditionChoice::Ladder(l) => ladder_rungs(output, &l.ladder)?,
        RenditionChoice::Preset(p) => {
            let Some(found) = preset(&p.preset) else {
                let ids: Vec<String> = builtin().into_iter().map(|p| p.id).collect();
                return Err(format!(
                    "There is no rendition preset called `{}`. Pick one of: {}.",
                    p.preset,
                    ids.join(", ")
                ));
            };
            if found.id == "copy" {
                return Ok(None);
            }
            match found.ladder {
                Some(rungs) => rungs
                    .into_iter()
                    .map(|r| RenditionRequest {
                        id: format!("{output}-{}", r.id),
                        ..r
                    })
                    .collect(),
                None => vec![RenditionRequest {
                    id: output.into(),
                    ..found.request
                }],
            }
        }
    };
    Ok(Some(rungs))
}

/// A custom ladder's rungs, each named `<output>-<its id>`, or after its
/// height (`<output>-480p`) when it has no id or shares one.
fn ladder_rungs(output: &str, rungs: &[RenditionRequest]) -> Result<Vec<RenditionRequest>, String> {
    if rungs.is_empty() {
        return Err("A ladder needs at least one rung. Add one, or pick a single format.".into());
    }
    let mut out: Vec<RenditionRequest> = Vec::with_capacity(rungs.len());
    for (i, r) in rungs.iter().enumerate() {
        let height = r.video.as_ref().and_then(|v| v.height);
        let own = Some(r.id.as_str())
            .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
        let mut name = own
            .map(str::to_string)
            .or(height.map(|h| format!("{h}p")))
            .unwrap_or(format!("rung{}", i + 1));
        if out.iter().any(|o| o.id == format!("{output}-{name}")) {
            name = format!("rung{}", i + 1);
        }
        out.push(RenditionRequest {
            id: format!("{output}-{name}"),
            ..r.clone()
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use godwinmix_protocol::rendition::PresetRef;

    fn by(id: &str) -> RenditionChoice {
        RenditionChoice::Preset(PresetRef { preset: id.into() })
    }

    #[test]
    fn a_ladder_gives_one_request_per_rung_named_after_the_output() {
        let reqs = expand("hls-main", &by("abr-ladder-4")).unwrap().unwrap();
        let ids: Vec<&str> = reqs.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["hls-main-1080p", "hls-main-720p", "hls-main-480p", "hls-main-360p"]);
    }

    #[test]
    fn a_single_preset_takes_the_output_id_and_copy_is_nothing() {
        let reqs = expand("yt", &by("youtube-720p30")).unwrap().unwrap();
        assert_eq!(reqs.len(), 1);
        assert_eq!(reqs[0].id, "yt");
        assert_eq!(expand("yt", &by("copy")).unwrap(), None);
    }

    #[test]
    fn a_custom_ladder_names_each_rung_and_reads_from_json() {
        let c: RenditionChoice = serde_json::from_str(
            r#"{"ladder":[{"id":"","container":"hls","video":{"height":720}},{"id":"","container":"hls","video":{"height":360}}]}"#,
        )
        .unwrap();
        let ids: Vec<String> = expand("hls", &c).unwrap().unwrap().into_iter().map(|r| r.id).collect();
        assert_eq!(ids, ["hls-720p", "hls-360p"]);
        let empty = RenditionChoice::Ladder(godwinmix_protocol::rendition::LadderRef { ladder: vec![] });
        assert!(expand("hls", &empty).is_err());
    }

    #[test]
    fn an_unknown_preset_names_the_ones_there_are() {
        let e = expand("yt", &by("youtube-8k")).unwrap_err();
        assert!(e.contains("abr-ladder-4"), "{e}");
    }

    #[test]
    fn a_preset_and_a_request_both_read_from_json() {
        let p: RenditionChoice = serde_json::from_str(r#"{"preset":"youtube-720p30"}"#).unwrap();
        assert_eq!(p, by("youtube-720p30"));
        let r: RenditionChoice = serde_json::from_str(
            r#"{"id":"x","container":"flv","video":{"codec":"h264","height":480}}"#,
        )
        .unwrap();
        assert!(matches!(r, RenditionChoice::Request(_)));
    }
}
