//! The import's report: counts, notes in the protocol's words, and no
//! password in any address it shows.

use super::*;
use godwinmix_flussonic::Document;

const CONFIG: &str = "http 8090;
weird_global 1;
stream NEWS {
  title News;
  input mpts-udp://eth1@239.10.0.1:2001?program=7;
  transcoder gop=50 hw=nvenc deinterlace=false vb=1500k vcodec=h264 size=1280x720:fit:#000000 ab=128k;
}
stream RELAY {
  disabled true;
  input rtmp://ingest:secret@127.0.0.1:1935/live/relay;
  input rtmp://ingest:secret@127.0.0.2:1935/live/relay;
}
";

fn made() -> HeadendImportReport {
    let doc = Document::parse(CONFIG).unwrap();
    report("pasted text".into(), true, import::import(&doc.config), ShowAddManyResult::default(), None)
}

#[test]
fn counts_each_stream_as_the_show_it_becomes() {
    let r = made();
    assert_eq!(r.streams.len(), 2);
    assert_eq!((r.converted, r.copied, r.disabled), (1, 1, 1));
    let news = &r.streams[0];
    assert_eq!(news.title.as_deref(), Some("News"));
    assert_eq!(news.input["program"], 7);
    assert_eq!(news.outputs[0]["params"]["segments"], "both");
    assert_eq!(news.outputs[0]["rendition"]["video"]["keyframe_frames"], 50);
    assert!(r.streams[1].copy && r.streams[1].disabled);
}

#[test]
fn every_line_not_carried_is_a_note_with_its_kind() {
    let r = made();
    let weird = r.notes.iter().find(|n| n.what.contains("weird_global")).expect("the unknown directive is listed");
    assert_eq!(weird.kind, "ignored");
    assert_eq!(weird.scope, "global");
    assert_eq!(weird.line, 2);
    assert!(r.notes.iter().all(|n| n.kind == "ignored" || n.kind == "not_carried"));
}

#[test]
fn no_password_is_shown_in_an_input_or_its_backup() {
    let r = made();
    let input = &r.streams[1].input;
    assert_eq!(input["uri"], "rtmp://ingest:***@127.0.0.1:1935/live/relay");
    assert_eq!(input["backup"]["uri"], "rtmp://ingest:***@127.0.0.2:1935/live/relay");
    // An input with no backup does not grow one.
    assert!(r.streams[0].input.get("backup").is_none());
    assert!(!serde_json::to_string(&r).unwrap().contains("secret"));
}

#[test]
fn the_station_answers_the_import_itself() {
    use crate::station::methods::answers;
    assert!(answers("headend.import", &serde_json::json!({"text": ""})), "it makes the shows");
    assert!(answers("headend.status", &serde_json::json!({})));
}
