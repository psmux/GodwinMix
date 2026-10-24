use super::*;

#[test]
fn a_line_is_in_flussonic_s_shape() {
    let l = line(1_792_756_804_123, "CH117", Event::Scrambled.text());
    assert_eq!(l, "2026-10-23 12:00:04.123 <0.0.0> [CH117] Input closed with reason => scrambled\n");
    // What a monitoring page looks for in each.
    assert!(Event::EncoderRefused.text().contains("open_session") && Event::EncoderRefused.text().contains("configure fail"));
    assert!(line(0, "A", "two\nlines").ends_with("[A] two lines\n"));
}

#[test]
fn the_file_rotates_and_never_grows_past_its_limits() {
    let dir = std::env::temp_dir().join(format!("gmx-flussonic-log-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("flussonic.log");
    let mut log = Log::with_limits(&path, 1000, 2);
    for _ in 0..100 {
        log.event("CH117", Event::Scrambled).unwrap();
    }
    let size = |p: &Path| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    assert!(size(&path) <= 1000 && size(&path) > 0);
    assert!(size(&dir.join("flussonic.log.1")) > 0 && size(&dir.join("flussonic.log.2")) > 0);
    assert!(!dir.join("flussonic.log.3").exists());
    let _ = std::fs::remove_dir_all(&dir);
}
