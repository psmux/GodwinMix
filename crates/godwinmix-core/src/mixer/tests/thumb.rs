use super::*;

/// Ask for the programme thumbnail on a running mixer with `graphics`, and
/// say what came back and how many frames the branch kept in three seconds,
/// times the timing slack.
async fn ask(graphics: crate::config::Accel) -> (crate::preview::thumb::Thumb, u64) {
    let mut cfg = programme_config(graphics);
    cfg.sources = vec![SourceConfig::bare("ball", "test://ball")];
    let (mut mix, handle, commands, _bus) = Mixer::build(cfg).unwrap();
    mix.start().unwrap();
    let preview = mix.preview_handle();
    let thread = spawn(mix, commands, handle.clone());
    let started = tokio::time::Instant::now();
    let thumb = loop {
        if let Some(t) = preview.programme_thumbnail(160).await.unwrap() {
            break t;
        }
        assert!(started.elapsed() < Duration::from_secs(10), "no programme thumbnail in 10 s");
    };
    let before = preview.thumb().frames();
    tokio::time::sleep(Duration::from_secs(3).mul_f64(slack())).await;
    let kept = preview.thumb().frames() - before;
    let _ = handle.send(Command::Shutdown);
    let _ = tokio::task::spawn_blocking(move || thread.join()).await;
    (thumb, kept)
}

/// The branch hangs off the raw tee and makes a picture at the asked width
/// in the canvas's shape, about one frame a second and no more.
#[tokio::test(flavor = "multi_thread")]
async fn the_programme_thumbnail_is_small_and_one_a_second() {
    gst::init().unwrap();
    let (t, kept) = ask(crate::config::Accel::Software).await;
    assert_eq!((t.width, t.height), (160, 90));
    assert_eq!(&t.jpeg[..2], &[0xff, 0xd8]);
    let most = (5.0 * slack()).round() as u64;
    assert!((1..=most).contains(&kept), "the branch kept {kept} frames in {:.0} seconds; it should keep about one a second", 3.0 * slack());
}

/// `GODWINMIX_TIMING_SLACK`: on a loaded Windows runner the branch kept no
/// frame in three seconds, and at one a second a few more seconds tell a
/// slow machine from a branch that has stopped.
fn slack() -> f64 {
    crate::plugin::harness::timing_slack()
}

/// The same on the GL entry, where the rate is cut before the download, so a
/// GPU programme comes down to memory once a second rather than every frame.
#[tokio::test(flavor = "multi_thread")]
async fn the_programme_thumbnail_comes_down_from_the_gpu() {
    gst::init().unwrap();
    if std::env::var_os("GST_GL_DISABLED").is_some()
        || (cfg!(target_os = "linux") && std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none())
        || !crate::probe::exists("glvideomixer")
        || !crate::probe::exists("gldownload")
    {
        eprintln!("skipping: no GL here");
        return;
    }
    let (t, kept) = ask(crate::config::Accel::Gl).await;
    assert_eq!((t.width, t.height), (160, 90));
    let most = (5.0 * slack()).round() as u64;
    assert!((1..=most).contains(&kept), "{kept} frames in {:.0} seconds", 3.0 * slack());
}
