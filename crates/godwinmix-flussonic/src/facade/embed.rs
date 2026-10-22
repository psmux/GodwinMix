//! `/<name>/embed.html`, a player page that loads nothing from anywhere
//! else, and the hls.js it plays with, served at `/flu/player/hls.min.js`
//! (the path Flussonic's own player files have, so a tunnel or proxy that
//! already passes them to this port needs no change).
//!
//! `assets/hls.min.js` is hls.js 1.5.20 as released (Apache License 2.0,
//! https://github.com/video-dev/hls.js).

pub const HLS_JS: &[u8] = include_bytes!("../../assets/hls.min.js");

/// The files under `/flu/player/`.
pub fn player_file(name: &str) -> Option<(&'static [u8], &'static str)> {
    match name {
        "hls.min.js" | "hls.js" => Some((HLS_JS, "application/javascript")),
        _ => None,
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// The page. `title` is the stream's title or name.
pub fn page(title: &str) -> String {
    let title = escape(title);
    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
  html, body {{ margin: 0; height: 100%; background: #000; }}
  video {{ width: 100%; height: 100%; object-fit: contain; background: #000; }}
  select {{ position: fixed; top: 8px; right: 8px; font: 14px system-ui, sans-serif; display: none; }}
</style>
</head>
<body>
<video id="v" controls autoplay muted playsinline></video>
<select id="sound" aria-label="Sound track"></select>
<script src="../flu/player/hls.min.js"></script>
<script>
  var video = document.getElementById("v"), menu = document.getElementById("sound");
  if (window.Hls && Hls.isSupported()) {{
    var hls = new Hls({{ liveSyncDurationCount: 3 }});
    hls.loadSource("index.m3u8");
    hls.attachMedia(video);
    hls.on(Hls.Events.AUDIO_TRACKS_UPDATED, function () {{
      menu.innerHTML = "";
      hls.audioTracks.forEach(function (t, i) {{
        var o = document.createElement("option");
        o.value = i; o.textContent = t.name || t.lang || ("Sound " + (i + 1));
        menu.appendChild(o);
      }});
      menu.value = hls.audioTrack;
      menu.style.display = hls.audioTracks.length > 1 ? "block" : "none";
    }});
    menu.onchange = function () {{ hls.audioTrack = Number(menu.value); }};
  }} else {{
    video.src = "index.m3u8";
  }}
</script>
</body>
</html>
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_plays_relative_and_loads_only_its_own_files() {
        let p = page("News <1>");
        assert!(p.contains("<title>News &lt;1&gt;</title>"));
        assert!(p.contains("loadSource(\"index.m3u8\")") && p.contains("src=\"../flu/player/hls.min.js\""));
        assert!(!p.contains("http://") && !p.contains("https://"));
        assert!(HLS_JS.len() > 100_000 && player_file("hls.min.js").is_some() && player_file("x.js").is_none());
    }
}
