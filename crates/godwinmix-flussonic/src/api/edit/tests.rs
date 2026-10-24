use super::*;
use crate::api::echo;
use serde_json::json;

const FILE: &str = "http 8090;\nedit_auth admin x;\n# kept\nstream A {\n  input udp://239.0.0.1:1;\n  weird 1;\n  transcoder vb=1m ab=64k;\n}\n";

#[test]
fn a_partial_body_changes_only_what_it_names() {
    let mut doc = Document::parse(FILE).unwrap();
    put(&mut doc, "A", &json!({"title": "Channel A", "inputs": [{"url": "rtmp://h/a"}, {"url": "udp://239.0.0.2:1"}]})).unwrap();
    let text = doc.text();
    assert_eq!(text, "http 8090;\nedit_auth admin x;\n# kept\nstream A {\n  input rtmp://h/a;\n  input udp://239.0.0.2:1;\n  weird 1;\n  transcoder vb=1m ab=64k;\n  title \"Channel A\";\n}\n");
    let s = &doc.config.streams[0];
    assert_eq!((s.title.as_deref(), s.inputs.len()), (Some("Channel A"), 2));
    put(&mut doc, "A", &json!({"transcoder": null, "disabled": true})).unwrap();
    assert!(doc.config.streams[0].transcoder.is_none() && doc.config.streams[0].disabled);
    assert!(doc.text().contains("weird 1;"));
}

#[test]
fn the_transcoder_the_api_echoes_writes_back_as_the_same_line() {
    let line = "deviceid=0 gop=50 hw=nvenc deinterlace=false vb=1500k vcodec=h264 size=1280x720:fit:#000000 ab=128k";
    let doc = Document::parse(&format!("stream A {{\n  input rtmp://h/a;\n  transcoder {line};\n}}\n")).unwrap();
    let echoed = echo::transcoder(doc.config.streams[0].transcoder.as_ref().unwrap());
    let mut again = Document::parse("stream A {\n  input rtmp://h/a;\n}\n").unwrap();
    put(&mut again, "A", &json!({"transcoder": echoed})).unwrap();
    assert_eq!(again.config.streams[0].transcoder, doc.config.streams[0].transcoder);
}

#[test]
fn a_new_stream_is_added_and_bad_bodies_are_refused() {
    let mut doc = Document::parse(FILE).unwrap();
    put(&mut doc, "B", &json!({"inputs": [{"url": "rtmp://h/b"}], "transcoder": "vb=800k ab=96k"})).unwrap();
    assert_eq!(doc.config.streams[1].name, "B");
    assert!(put(&mut doc, "C", &json!({"inputs": [{"url": "nope"}]})).is_err());
    let e = put(&mut doc, "A", &json!({"dvr": "x"})).unwrap_err();
    assert!(e.to_string().contains("'dvr' cannot be set here"), "{e}");
    assert!(put(&mut doc, "A", &json!({"name": "Z"})).is_err());
    assert_eq!(doc.config.streams.len(), 2, "a refused body changes nothing");
}
