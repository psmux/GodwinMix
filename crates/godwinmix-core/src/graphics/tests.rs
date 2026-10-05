//! Templates drawn for real, with `rsvgdec`.

use super::*;
use crate::overlay::blend::Rect;
use template::TemplateOrigin;

fn have_rsvg() -> bool {
    let _ = gstreamer::init();
    let ok = crate::probe::exists("rsvgdec");
    if !ok {
        println!("skipping: no rsvgdec in this GStreamer");
    }
    ok
}

fn values(pairs: &[(&str, &str)]) -> Values {
    pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
}

/// Columns with any ink in rows `y0..y1` of a picture, first and last.
fn ink(pic: &crate::overlay::Picture, y0: u32, y1: u32) -> Option<(u32, u32)> {
    let map = pic.buffer.map_readable().unwrap();
    let mut span: Option<(u32, u32)> = None;
    for y in y0..y1.min(pic.height) {
        for x in 0..pic.width {
            if map[y as usize * pic.stride + x as usize * 4] > 8 {
                span = Some(span.map_or((x, x), |(a, b)| (a.min(x), b.max(x))));
            }
        }
    }
    span
}

#[test]
fn every_template_in_the_pack_loads_and_renders_with_its_defaults() {
    if !have_rsvg() {
        return;
    }
    let pack = pack::pack();
    assert_eq!(pack.len(), pack::PACK.len(), "every pack file reads as a template");
    for t in pack {
        assert_eq!((t.info.width, t.info.height), (1920, 1080), "{} is laid out on the whole canvas", t.info.name);
        for f in &t.info.fields {
            assert!(!f.default.is_empty(), "{} field {} has a default to show", t.info.name, f.name);
        }
        for brand in template::BRAND {
            assert!(t.field(brand).is_some_and(|f| f.kind == template::FieldType::Color), "{} has the {brand} colour", t.info.name);
        }
        let pic = render(&t, &Values::new(), (960, 540)).unwrap_or_else(|e| panic!("{} renders: {e:#}", t.info.name));
        assert_eq!((pic.width, pic.height, pic.natural), (960, 540, (1920, 1080)));
        let c = pic.content.expect("the content is measured");
        assert!(c.w > 20 && c.h > 10, "{} draws something: {c:?}", t.info.name);
        // A full screen card and a set's foreground run to the edges.
        if t.info.name != "title-card" && !t.info.name.starts_with("set-") {
            // Inside title safe, the inner 90 percent, at half size.
            let safe = Rect::new(48, 27, 864, 486);
            assert_eq!(c.within(&safe), Some(c), "{} stays inside title safe: {c:?}", t.info.name);
        }
    }
}

#[test]
fn xml_special_characters_in_a_field_render_as_text_and_change_nothing_else() {
    if !have_rsvg() {
        return;
    }
    let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 100"><text x="10" y="60" font-size="30" fill="#ffffff">{{headline}}</text></svg>"##;
    let t = Template::parse("t", TemplateOrigin::Library, svg.into()).unwrap();
    let nasty = r#"</text><rect width="400" height="100" fill="red"/><text>Tom & Jerry's "<b>""#;
    let v = values(&[("headline", nasty)]);
    let filled = fill::fill(&t, &v, &BrandConfig::default());
    assert!(filled.contains("Tom &amp; Jerry&apos;s &quot;&lt;b&gt;&quot;"), "{filled}");
    assert!(!filled.contains("<rect"), "the value could not add an element");
    let pic = render(&t, &v, (400, 100)).expect("a value with markup in it still renders");
    let c = pic.content.unwrap();
    assert!(c.w < 400 && c.h < 60, "only the words are drawn, no red rectangle over the frame: {c:?}");
    let plain = render(&t, &values(&[("headline", "Tom")]), (400, 100)).unwrap();
    assert!(c.w > plain.content.unwrap().w * 3, "the markup is shown as letters: {c:?}");
}

#[test]
fn shrink_to_fit_keeps_a_long_headline_inside_its_box() {
    if !have_rsvg() {
        return;
    }
    let svg = |fit: &str| {
        format!(r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1000 100"><text x="100" y="70" font-size="48" fill="#fff" {fit}>{{{{headline}}}}</text></svg>"##)
    };
    let long = values(&[("headline", "A very long headline that would run far past the edge of its panel")]);
    let free = Template::parse("free", TemplateOrigin::Library, svg("")).unwrap();
    let (_, right) = ink(&render(&free, &long, (1000, 100)).unwrap(), 0, 100).unwrap();
    assert!(right > 600, "without fitting the words run on: right edge {right}");
    let fitted = Template::parse("fitted", TemplateOrigin::Library, svg(r#"data-fit-width="400""#)).unwrap();
    assert_eq!(fitted.field("headline").and_then(|f| f.fit), Some(400.0), "the field says it is fitted");
    let pic = render(&fitted, &long, (1000, 100)).unwrap();
    let (left, right) = ink(&pic, 0, 100).unwrap();
    assert!((95..=110).contains(&left), "it keeps its left edge: {left}");
    assert!(right <= 504 && right > 440, "it fits inside 100 to 500: right edge {right}");
    let short = render(&fitted, &values(&[("headline", "Short")]), (1000, 100)).unwrap();
    let (_, r) = ink(&short, 0, 100).unwrap();
    assert!(r < 400, "a short one is left at its size: {r}");
}

#[test]
fn a_brand_colour_fills_the_field_when_the_source_does_not() {
    let t = pack::load("breaking-news").unwrap();
    let brand = BrandConfig { accent: "#00ff00".into(), ..BrandConfig::default() };
    let filled = fill::fill(&t, &Values::new(), &brand);
    assert!(filled.contains(r##"fill="#00ff00""##), "the brand accent is used");
    let own = fill::fill(&t, &values(&[("accent", "#0000ff")]), &brand);
    assert!(own.contains(r##"fill="#0000ff""##) && !own.contains("#00ff00"), "the source's own value wins");
}

#[test]
fn an_unknown_field_names_the_fields_the_template_has() {
    let t = pack::load("news-lower-third").unwrap();
    let e = fill::check(&t, &values(&[("headline", "x")])).unwrap_err();
    let unknown = e.downcast_ref::<UnknownField>().expect("a typed error a caller can read");
    assert_eq!(unknown.fields, vec!["name", "title", "accent", "text", "panel"]);
    assert!(e.to_string().contains("name, title"), "{e}");
    let e = fill::check(&t, &values(&[("accent", "red")])).unwrap_err().to_string();
    assert!(e.contains("params.fields.accent") && e.contains("#rrggbb"), "{e}");
}
