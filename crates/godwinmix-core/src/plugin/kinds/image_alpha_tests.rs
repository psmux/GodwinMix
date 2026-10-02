use super::*;

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("gmx-alpha-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn params(uri: &str) -> Params {
    let mut p = Params::new();
    p.insert("uri".into(), toml::Value::String(uri.into()));
    p
}

#[test]
fn an_svg_renders_at_the_size_asked_and_keeps_its_own_shape() {
    let _ = gst::init();
    if !crate::probe::exists("rsvgdec") {
        println!("skipping: no rsvgdec");
        return;
    }
    let dir = scratch("svg");
    let svg = dir.join("mark.svg");
    std::fs::write(&svg, r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="20" height="20" fill="#ff0000"/></svg>"##).unwrap();
    let still = AlphaStill::validate(&params(&svg.to_string_lossy())).unwrap();
    let own = still.render(None).unwrap().picture.unwrap();
    assert_eq!((own.width, own.height, own.natural), (40, 20, (40, 20)));
    let big = still.render(Some((160, 80))).unwrap().picture.unwrap();
    assert_eq!((big.width, big.height, big.natural), (160, 80, (40, 20)));
    let map = big.buffer.map_readable().unwrap();
    let at = |x: usize, y: usize| map[y * big.stride + x * 4];
    assert_eq!(at(40, 40), 255, "the red square is opaque");
    assert_eq!(at(120, 40), 0, "the rest is clear");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn alpha_is_asked_for_by_params_or_found_in_the_file() {
    let mut p = params("x.png");
    assert!(!wanted("/nowhere/x.png", &p).unwrap(), "a file that is not there is taken as opaque");
    assert!(wanted("/nowhere/x.svg", &p).unwrap(), "an SVG is always transparent");
    p.insert("alpha".into(), toml::Value::Boolean(true));
    assert!(wanted("/nowhere/x.png", &p).unwrap());
    p.insert("alpha".into(), toml::Value::Integer(2));
    assert!(wanted("/nowhere/x.png", &p).unwrap_err().to_string().contains("true, false or \"auto\""));
}
