use super::*;
use serde_json::Value;

#[test]
fn a_csv_with_a_header_is_read_by_column_name() {
    let text = "input,name,outputs,program,notes\n\
                udp://@239.1.1.1:5000,BBC One,srt://out:9001;udp://10.0.0.9:6000,101,hd\n\
                \n\
                # a comment\n\
                udp://@239.1.1.2:5000,BBC Two,,,\n";
    let feeds = parse(text).unwrap();
    assert_eq!(feeds.len(), 2);
    assert_eq!(feeds[0].name, "BBC One");
    assert_eq!(feeds[0].input, "udp://@239.1.1.1:5000");
    assert_eq!(feeds[0].program, Some(101));
    assert_eq!(feeds[0].outputs, vec!["srt://out:9001", "udp://10.0.0.9:6000"]);
    assert_eq!(feeds[1].program, None);
    assert!(feeds[1].outputs.is_empty());
    assert_eq!(feeds[1].line, 5);
}

#[test]
fn one_address_per_line_names_each_feed_after_its_address() {
    let feeds = parse("udp://@239.1.1.7:5000\nsrt://enc.local:9000?latency=200\n").unwrap();
    assert_eq!(feeds[0].name, "239-1-1-7-5000");
    assert_eq!(feeds[1].name, "enc-local-9000");
    assert_eq!(feeds[1].input, "srt://enc.local:9000?latency=200");
}

#[test]
fn a_line_without_a_header_is_name_input_program_outputs() {
    let feeds = parse("\"News, late\",udp://@239.1.1.9:5000,3,srt://out:9000").unwrap();
    assert_eq!(feeds[0].name, "News, late");
    assert_eq!(feeds[0].program, Some(3));
    let show = feeds[0].to_show(false);
    assert_eq!(show["compositing"], false);
    assert_eq!(show["input"]["program"], 3);
    assert_eq!(show["outputs"][0]["uri"], "srt://out:9000");
}

#[test]
fn a_mistake_names_its_line_and_what_to_write() {
    let err = parse("name,input\nBBC One,not an address\n").unwrap_err().to_string();
    assert!(err.contains("line 2") && err.contains("udp://"), "{err}");
    let err = parse("x,udp://@239.1.1.1:5000,one").unwrap_err().to_string();
    assert!(err.contains("line 1") && err.contains("program"), "{err}");
    assert!(parse("# nothing\n\n").is_err());
}

#[test]
fn the_list_gmx_scale_writes_is_read_with_its_format() {
    let text = "name,input,program,output,format\n\
                feed-001,udp://@239.77.0.1:5000,1,udp://127.0.0.1:30000,copy\n\
                feed-002,udp://@:20001,1,udp://127.0.0.1:30001,youtube-720p30\n";
    let feeds = parse(text).unwrap();
    assert_eq!(feeds[0].outputs, vec!["udp://127.0.0.1:30000"]);
    assert_eq!(feeds[0].to_show(false)["outputs"][0]["rendition"], Value::Null);
    let show = feeds[1].to_show(false);
    assert_eq!(show["outputs"][0]["rendition"]["preset"], "youtube-720p30");
    assert_eq!(show["input"]["program"], 1);
}
