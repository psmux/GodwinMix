//! The shared cutout model: who shares, who feeds, and a change of settings.

use super::*;

/// No model is needed: a worker that cannot open one still takes offers
/// and still decides whether to.
#[test]
fn the_same_picture_and_masks_share_one_worker_and_others_do_not() {
    let s = Settings::default();
    let a = worker_for(Some("share-test-cam"), &s);
    let b = worker_for(Some("share-test-cam"), &s);
    assert!(Arc::ptr_eq(&a, &b), "two cutouts on one picture ran two models");
    let other = worker_for(Some("share-test-other"), &s);
    assert!(!Arc::ptr_eq(&a, &other), "two pictures shared one model");
    let steadier = Settings { steady: 0.9, ..s.clone() };
    let c = worker_for(Some("share-test-cam"), &steadier);
    assert!(!Arc::ptr_eq(&a, &c), "a cutout asking other masks was given these");
    let edge = Settings { cutoff: 0.3, feather: 4, ..s.clone() };
    assert!(Arc::ptr_eq(&a, &worker_for(Some("share-test-cam"), &edge)), "the edge is not the model's");
    let alone = worker_for(None, &s);
    assert!(!Arc::ptr_eq(&a, &alone));
}

/// The review's case: one item on air and in Preview, its cutout made
/// steadier. Preview is told first and moves to a new model; Programme,
/// told second, must follow it rather than change the old one in place,
/// or the camera ends on two models. Then the other order.
#[test]
fn a_change_of_settings_told_to_both_copies_ends_on_one_model() {
    for (first, key) in [("preview", "share-test-settle-a"), ("programme", "share-test-settle-b")] {
        let s = Settings::default();
        let mut programme = worker_for(Some(key), &s);
        let mut preview = worker_for(Some(key), &s);
        assert!(Arc::ptr_eq(&programme, &preview));
        let steadier = Settings { steady: 0.9, ..s.clone() };
        let (a, b) = if first == "preview" { (&mut preview, &mut programme) } else { (&mut programme, &mut preview) };
        let mine = a.clone();
        *a = settle(Some(key), &mine, &steadier).expect("a shared worker is not changed under the other copy");
        drop(mine);
        let mine = b.clone();
        if let Some(next) = settle(Some(key), &mine, &steadier) {
            *b = next;
        } else {
            b.set(steadier.clone());
        }
        drop(mine);
        assert!(Arc::ptr_eq(&programme, &preview), "{first} told first left two models on one camera");
        assert_eq!(workers_on(key), 1, "{first} told first: the old model is still running");
    }
}

/// A cutout alone on its worker is changed in place, so its last mask
/// stays up while the model reloads.
#[test]
fn a_worker_alone_is_changed_in_place() {
    let s = Settings::default();
    let w = worker_for(Some("share-test-alone"), &s);
    let mine = w.clone();
    assert!(settle(Some("share-test-alone"), &mine, &Settings { steady: 0.9, ..s }).is_none());
}

#[test]
fn a_follower_waits_while_the_lead_feeds() {
    gstreamer::init().unwrap();
    let w = worker_for(Some("share-test-follow"), &Settings { quality: crate::plugin::filters::matte::params::Quality::Named("nothing-here".into()), ..Settings::default() });
    let info = gstreamer_video::VideoInfo::builder(gstreamer_video::VideoFormat::I420, 16, 16).build().unwrap();
    let frame = gstreamer::Buffer::with_size(info.size()).unwrap();
    w.offer_as(&frame, &info, false, 1);
    assert!(w.shared.lead_at.lock().is_none(), "a follower is not the lead");
    w.offer_as(&frame, &info, true, 2);
    w.shared.job.lock().take();
    w.offer_as(&frame, &info, false, 1);
    assert!(w.shared.job.lock().is_none(), "a follower's frame went in while the lead was feeding");
    w.offer_as(&frame, &info, true, 3);
    assert!(w.shared.job.lock().is_none(), "a second lead fed the model while the first was feeding");
    w.offer_as(&frame, &info, true, 2);
    assert!(w.shared.job.lock().is_some(), "the lead's own frame was dropped");
}
