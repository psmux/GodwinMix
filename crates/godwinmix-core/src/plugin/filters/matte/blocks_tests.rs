use super::*;

fn identity() -> [u8; 256] {
    std::array::from_fn(|i| i as u8)
}

#[test]
fn a_mask_half_on_lands_on_the_matching_half_of_the_frame() {
    // A 4 x 2 mask, left half 0, right half 255, over an 8 x 4 frame.
    let mask = [0, 0, 255, 255, 0, 0, 255, 255];
    let r = Region {
        x: 0,
        y: 0,
        w: 8,
        h: 4,
    };
    let mut out = Vec::new();
    alpha(&mask, (4, 2), (8, 4), r, &identity(), &mut out);
    assert_eq!(out.len(), 8);
    assert!(out[0] < 10 && out[3] > 245, "{out:?}");
    assert_eq!(out[0..4], out[4..8], "both rows the same");
}

/// What a 1080p frame costs on its own thread: the mask laid over it and
/// the frame's chroma copied. `cargo test --release -- --ignored cost`.
#[test]
#[ignore]
fn cost_per_1080p_frame() {
    let (w, h) = (1920usize, 1080usize);
    let mask: Vec<u8> = (0..512 * 288).map(|i| (i % 256) as u8).collect();
    let (u, v) = (
        vec![100u8; (w / 2) * (h / 2)],
        vec![150u8; (w / 2) * (h / 2)],
    );
    let r = Region { x: 0, y: 0, w, h };
    let (mut a, mut c) = (Vec::new(), Vec::new());
    let curve = identity();
    let n = 200;
    let t = std::time::Instant::now();
    for _ in 0..n {
        alpha(&mask, (512, 288), (w, h), r, &curve, &mut a);
        chroma(&u, &v, [w, w / 2, w / 2], r, &mut c);
    }
    let ms = t.elapsed().as_secs_f64() * 1000.0 / n as f64;
    eprintln!("cutout blocks at 1080p: {ms:.2} ms a frame");
    assert!(ms < 8.0, "{ms} ms");
}

#[test]
fn no_mask_yet_shows_the_whole_picture() {
    let r = Region {
        x: 0,
        y: 0,
        w: 4,
        h: 4,
    };
    let mut out = Vec::new();
    alpha(&[], (0, 0), (4, 4), r, &identity(), &mut out);
    assert_eq!(out, [255; 4]);
}

#[test]
fn flattening_takes_what_is_cut_out_to_black() {
    let (mut y, mut u, mut v) = (vec![200u8; 16], vec![60u8; 4], vec![200u8; 4]);
    let r = Region {
        x: 0,
        y: 0,
        w: 4,
        h: 4,
    };
    flatten(
        &mut y,
        &mut u,
        &mut v,
        [4, 2, 2],
        (4, 4),
        &[0, 255, 255, 255],
        r,
    );
    assert_eq!((y[0], u[0], v[0]), (16, 128, 128));
    assert_eq!((y[2], u[1]), (200, 60));
}
