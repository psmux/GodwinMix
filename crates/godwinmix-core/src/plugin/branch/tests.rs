use super::*;

fn queue_secs(q: &gst::Element) -> f64 {
    q.property::<u64>("max-size-time") as f64 / 1e9
}

/// The queue inside a `proxysrc`, which is a bin.
fn proxy_queue(branch: &ProgrammeBranch) -> gst::Element {
    let src = &branch.elements[0];
    let bin = src.downcast_ref::<gst::Bin>().expect("proxysrc is a bin");
    bin.iterate_elements()
        .into_iter()
        .flatten()
        .find(|e| e.factory().is_some_and(|f| f.name() == "queue"))
        .expect("proxysrc has a queue inside")
}

fn branch(id: &str, finite: bool) -> ProgrammeBranch {
    let _ = gst::init();
    let program = gst::Pipeline::new();
    let amix = make("audiomixer", "amix").unwrap();
    program.add(&amix).unwrap();
    let canvas = CanvasCaps::new(&crate::config::Canvas {
        width: 320,
        height: 180,
        fps: 15,
        sample_rate: 48000,
        channels: 2,
    });
    let (v, a) = (make("proxysink", "v").unwrap(), make("proxysink", "a").unwrap());
    let ctx = BranchCtx { program: &program, amix: &amix, canvas: &canvas };
    ProgrammeBranch::build(&ctx, &id.to_string(), &v, &a, 1.0, false, finite).unwrap()
}

/// A clip decodes as fast as it is let and fills every queue it meets, so
/// its queues are a tenth of a second where a live source's are a second.
#[test]
fn a_clip_waits_a_tenth_of_a_second_where_a_live_source_waits_one() {
    let clip = branch("clip", true);
    assert_eq!(queue_secs(&clip.vq), FINITE_QUEUE_SECS);
    assert_eq!(queue_secs(&proxy_queue(&clip)), FINITE_QUEUE_SECS);
    let camera = branch("camera", false);
    assert_eq!(queue_secs(&camera.vq), 1.0);
    assert!(queue_secs(&proxy_queue(&camera)) >= 1.0, "a live source keeps the proxy's own queue");
}
