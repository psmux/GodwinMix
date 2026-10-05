use super::*;

const GOOD: &str = r##"<!doctype html><html><head>
<script type="application/json" id="gmx-template">
{"title": "Strap", "out_ms": 500,
 "fields": {"name": {"label": "Name", "default": "Ada"}, "accent": {"type": "color", "default": "#c8102e"}}}
</script>
<style>html, body { background: transparent; margin: 0 }
.bar { background: var(--accent); transform: translateX(-110%); transition: transform .5s }
.gmx-in .bar { transform: none }</style></head>
<body><div class="bar" data-field="name">Ada</div></body></html>"##;

fn errors(html: &str) -> Vec<String> {
    check(html).0.into_iter().filter(|p| p.level == "error").map(|p| format!("{} | {}", p.problem, p.fix)).collect()
}

#[test]
fn a_good_template_has_no_problems_and_lists_its_fields_in_order() {
    let (problems, meta) = check(GOOD);
    assert!(problems.is_empty(), "{problems:?}");
    let meta = meta.unwrap();
    let names: Vec<&str> = meta.fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["name", "accent"]);
    assert_eq!(meta.out_ms, Some(500));
}

#[test]
fn a_page_that_paints_its_background_is_refused_with_the_line_to_write() {
    let e = errors(&GOOD.replace("background: transparent", "background: #000"));
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].contains("html, body { background: transparent") && e[0].contains("opaque"), "{e:?}");
    let inline = GOOD.replace("<body>", "<body style=\"background-color: white\">");
    assert_eq!(errors(&inline).len(), 1);
    let opaque = GOOD.replace("\"out_ms\": 500", "\"opaque\": true").replace("background: transparent", "background: #000");
    assert!(errors(&opaque).is_empty());
    assert!(errors(&GOOD.replace("background: transparent", "background: rgba(0, 0, 0, 0)")).is_empty());
}

#[test]
fn a_field_on_the_page_that_is_not_declared_names_the_ones_that_are() {
    let e = errors(&GOOD.replace("data-field=\"name\"", "data-field=\"title\""));
    assert!(e.iter().any(|m| m.contains("\"title\"") && m.contains("name, accent")), "{e:?}");
}

#[test]
fn the_network_and_a_missing_block_are_errors() {
    let e = errors(&GOOD.replace("</head>", "<link href=\"https://fonts.googleapis.com/css2?family=Inter\" rel=\"stylesheet\"></head>"));
    assert!(e.iter().any(|m| m.contains("fonts.googleapis.com") && m.contains("offline")), "{e:?}");
    let svg_ns = GOOD.replace("<body>", "<body><svg xmlns=\"http://www.w3.org/2000/svg\"></svg>");
    assert!(errors(&svg_ns).is_empty());
    let e = errors(&GOOD.replace("id=\"gmx-template\"", "id=\"other\""));
    assert!(e.iter().any(|m| m.contains("gmx-template")), "{e:?}");
    let e = errors(&GOOD.replace("\"title\": \"Strap\",", "\"title\": 'Strap',"));
    assert!(e.iter().any(|m| m.contains("not JSON")), "{e:?}");
}

#[test]
fn no_way_in_and_no_out_ms_are_warnings() {
    let (p, _) = check(&GOOD.replace(".gmx-in .bar { transform: none }", ""));
    assert!(p.iter().any(|p| p.level == "warning" && p.problem.contains(".gmx-in")), "{p:?}");
    let (p, _) = check(&GOOD.replace("\"out_ms\": 500,", ""));
    assert!(p.iter().any(|p| p.level == "warning" && p.problem.contains("out_ms")), "{p:?}");
}
