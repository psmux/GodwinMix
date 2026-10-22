use super::*;

fn stream(pid: u16, stream_type: u8, info: Vec<u8>) -> Stream {
    Stream { pid, stream_type, info }
}

fn eng() -> Vec<u8> {
    vec![0x0A, 4, b'E', b'N', b'G', 0]
}

/// An ATSC AC-3 descriptor for a stereo main service with language "spa".
fn atsc_spa() -> Vec<u8> {
    vec![0x81, 10, 0x08, 0x3C, 0x05, 0x00, 0x00, 0x00, 0x80, b's', b'p', b'a']
}

#[test]
fn sound_tracks_are_numbered_in_pmt_order_only_when_every_track_is_carried() {
    let pmt = [stream(0x31, 0x02, vec![]), stream(0x34, 0x81, atsc_spa()), stream(0x35, 0x81, eng()), stream(0x36, 0x06, vec![0x56, 0])];
    let all = pick(&pmt, Carry { mpeg2: true, tracks: true });
    let carried: Vec<_> = all.iter().map(|p| p.carried).collect();
    assert_eq!(carried, vec![Some((Codec::Mpeg2, 0)), Some((Codec::Ac3, 0)), Some((Codec::Ac3, 1)), None]);
    let tracks = infos(&all);
    assert_eq!(tracks[1].lang.as_deref(), Some("spa"), "from the ATSC descriptor");
    assert_eq!(tracks[2].lang.as_deref(), Some("eng"), "lower case");
    let first = pick(&pmt, Carry::default());
    assert_eq!(first[0].note.as_deref(), Some("mpeg2 video, which a direct show does not carry"));
    assert_eq!(first[2].note.as_deref(), Some("a second audio stream (ac3); the first is taken"));
    assert_eq!(infos(&first).len(), 1, "the first sound alone");
}
