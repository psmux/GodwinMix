//! What an `hls/output` is asked to serve: the programme as it is, a named
//! ladder, or a ladder given rung by rung as rendition requests.

use super::ladder::{self, Rung};
use crate::config::Params;
use anyhow::{Context, Result};
use godwinmix_protocol::rendition::{RenditionRequest, VideoCodec};

/// The ladder an output's params ask for: `rendition = { preset =
/// "abr-ladder-4" }`, `rendition = { ladder = [RenditionRequest, ...] }`, or
/// `ladder = "abr-ladder-4"` for short. None serves the programme as it is.
pub fn ladder_of(params: &Params) -> Result<Option<Vec<Rung>>> {
    let rendition = params.get("rendition");
    if let Some(list) = rendition.and_then(|r| r.get("ladder")) {
        return custom_ladder(list).map(Some);
    }
    let named = params
        .get("ladder")
        .and_then(|v| v.as_str())
        .or_else(|| rendition.and_then(|r| r.get("preset")).and_then(|v| v.as_str()));
    let Some(name) = named.filter(|n| *n != "copy") else { return Ok(None) };
    ladder::preset(name).map(Some).with_context(|| {
        format!(
            "hls/output has no ladder called `{name}`. It has abr-ladder-4 (1080p, 720p, 480p, \
             360p) and abr-ladder-3 (720p, 480p, 360p), or a ladder of your own as rendition.ladder; \
             leave it out to serve the programme as it is."
        )
    })
}

/// A ladder given rung by rung, as the page's custom ladder sends it.
fn custom_ladder(list: &toml::Value) -> Result<Vec<Rung>> {
    let requests: Vec<RenditionRequest> = list.clone().try_into().context(
        "hls/output rendition.ladder must be a list of renditions, each with an id and video.height",
    )?;
    anyhow::ensure!(
        !requests.is_empty(),
        "hls/output rendition.ladder is empty. Give it at least one rung, or leave it out to serve the programme as it is."
    );
    requests.iter().map(rung_of).collect()
}

fn rung_of(r: &RenditionRequest) -> Result<Rung> {
    let slug = !r.id.is_empty() && r.id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    anyhow::ensure!(slug, "rung id `{}` must be a short slug of small letters, numbers and dashes, like 720p", r.id);
    anyhow::ensure!(r.id != "audio", "`audio` is the name of the sound every rung shares. Call the rung by its size, like 720p.");
    let v = r.video.as_ref().with_context(|| format!("rung {} says nothing about its video. Give it video.height at least.", r.id))?;
    if let Some(c) = v.codec.filter(|c| *c != VideoCodec::H264) {
        anyhow::bail!(
            "rung {} asks for {c:?}; a ladder the HLS output makes itself is H.264 for now. Ask for h264, or leave the codec out.",
            r.id
        );
    }
    let height = v.height.with_context(|| format!("rung {} needs video.height", r.id))?;
    anyhow::ensure!((144..=2160).contains(&height), "rung {} is {height} lines high; a ladder takes 144 to 2160", r.id);
    let width = v.width.unwrap_or(height * 16 / 9);
    let kbps = v.bitrate_kbps.unwrap_or(match height {
        h if h >= 1080 => 6000,
        h if h >= 720 => 3000,
        h if h >= 480 => 1500,
        _ => 800,
    });
    Ok(Rung::new(&r.id, (width + 1) & !1, (height + 1) & !1, kbps))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(text: &str) -> Params {
        toml::from_str(text).unwrap()
    }

    #[test]
    fn nothing_asked_is_the_programme_as_it_is() {
        assert_eq!(ladder_of(&Params::new()).unwrap(), None);
        assert_eq!(ladder_of(&params("rendition = { preset = \"copy\" }")).unwrap(), None);
    }

    #[test]
    fn a_preset_by_either_name() {
        let four = ladder_of(&params("rendition = { preset = \"abr-ladder-4\" }")).unwrap().unwrap();
        assert_eq!(four.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["1080p", "720p", "480p", "360p"]);
        assert_eq!(ladder_of(&params("ladder = \"abr-ladder-3\"")).unwrap().unwrap().len(), 3);
        let e = ladder_of(&params("ladder = \"abr-ladder-9\"")).unwrap_err().to_string();
        assert!(e.contains("abr-ladder-4"), "{e}");
    }

    #[test]
    fn a_custom_ladder_of_rendition_requests() {
        let text = r#"
            [[rendition.ladder]]
            id = "540p"
            container = "hls"
            video = { codec = "h264", height = 540, bitrate_kbps = 2000, keyframe_ms = 2000 }
            audio = { codec = "aac", bitrate_kbps = 128 }
            [[rendition.ladder]]
            id = "240p"
            container = "hls"
            video = { height = 240 }
        "#;
        let rungs = ladder_of(&params(text)).unwrap().unwrap();
        assert_eq!(rungs[0], Rung::new("540p", 960, 540, 2000));
        assert_eq!(rungs[1], Rung::new("240p", 426, 240, 800));
    }

    #[test]
    fn a_custom_rung_is_refused_with_the_reason() {
        let bad = |video: &str, id: &str| {
            let text = format!("[[rendition.ladder]]\nid = \"{id}\"\ncontainer = \"hls\"\nvideo = {video}\n");
            ladder_of(&params(&text)).unwrap_err().to_string()
        };
        assert!(bad("{ codec = \"h265\", height = 720 }", "720p").contains("H.264"));
        assert!(bad("{ height = 99 }", "tiny").contains("144 to 2160"));
        assert!(bad("{ width = 640 }", "w").contains("video.height"));
        assert!(bad("{ height = 720 }", "Big Rung").contains("slug"));
        assert!(bad("{ height = 720 }", "audio").contains("shares"));
    }
}
