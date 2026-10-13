//! Every module the page imports is served. 0.3.9 added
//! `panels/channels/sends.js` without a line in `ASSETS`, so the browser got a
//! 404 and the Channels panel never loaded; nothing failed until a person
//! opened it. This reads each served script, follows every relative import,
//! static or `import()`, and names the one that is missing.

use super::{ASSETS, DEV_ASSETS};

/// `../../shell/dom.js` from `panels/sources/panel.js` is `shell/dom.js`.
fn joined(from: &str, spec: &str) -> String {
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for piece in spec.split('/') {
        match piece {
            "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// The relative specifiers in a script: after `from`, after a bare `import`,
/// and inside `import(`. A specifier built at run time is not seen.
fn relative_imports(source: &str) -> Vec<&str> {
    let mut out = Vec::new();
    for marker in [" from \"", " from '", "import(\"", "import('", "import \"", "import '"] {
        let quote = &marker[marker.len() - 1..];
        let mut rest = source;
        while let Some(at) = rest.find(marker) {
            rest = &rest[at + marker.len()..];
            let Some(end) = rest.find(quote) else { break };
            let spec = &rest[..end];
            if spec.starts_with("./") || spec.starts_with("../") {
                out.push(spec);
            }
        }
    }
    out
}

#[test]
fn every_module_a_served_script_imports_is_served_too() {
    let served: std::collections::HashSet<&str> = ASSETS.iter().chain(DEV_ASSETS).map(|(p, _)| *p).collect();
    let mut missing = Vec::new();
    for (path, body) in ASSETS.iter().chain(DEV_ASSETS) {
        if !path.ends_with(".js") || path.contains("vendor/") {
            continue;
        }
        for spec in relative_imports(body) {
            let target = joined(path, spec);
            if !served.contains(target.as_str()) {
                missing.push(format!("{path} imports {spec}, and {target} is not in ASSETS"));
            }
        }
    }
    assert!(missing.is_empty(), "add these to the table in ui.rs:\n{}", missing.join("\n"));
}
