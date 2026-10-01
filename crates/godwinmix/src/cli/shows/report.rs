//! What `gmx shows add` prints from the answer to `show.add_many`.

use serde_json::Value;

/// What `show.add_many` said, in sentences: what went in, what was refused
/// and why (by the line of the file it came from), and the cost.
pub fn outcome(answer: &Value, feeds: &[super::feeds::Feed], dry_run: bool) -> String {
    let added = answer["added"].as_array().map(Vec::len).unwrap_or(0);
    let refused = answer["refused"].as_array().cloned().unwrap_or_default();
    let mut out = String::new();
    let verb = if dry_run { "would add" } else { "added" };
    out.push_str(&format!("{verb} {added} of {} shows\n", feeds.len()));
    for r in &refused {
        let index = r["index"].as_u64().unwrap_or(0) as usize;
        let line = feeds.get(index).map(|f| f.line).unwrap_or(0);
        let name = r["name"].as_str().unwrap_or("?");
        out.push_str(&format!("  line {line} {name}: {}\n", r["why"].as_str().unwrap_or("refused")));
    }
    let plan = &answer["plan"];
    if !plan.is_null() {
        let fits = if plan["fits"].as_bool() == Some(false) { "does not fit" } else { "fits" };
        out.push_str(&format!("cost {}, {fits} on this machine\n", compact(&plan["cost"])));
    }
    if dry_run && refused.is_empty() {
        out.push_str("nothing was changed. Run it again without --dry-run to add them.\n");
    }
    out
}

/// The governor's cost in words: `1.2 cores, 340 MiB, 24000 kbps out`.
fn compact(v: &Value) -> String {
    let Some(map) = v.as_object() else {
        return v.as_str().map(String::from).unwrap_or_else(|| "unknown".into());
    };
    let n = |k: &str| map.get(k).and_then(Value::as_u64).unwrap_or(0);
    format!(
        "{:.1} cores, {} MiB, {} kbps out",
        n("cpu_millicores") as f64 / 1000.0,
        n("memory_mib"),
        n("egress_kbps")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_refusal_is_reported_by_the_line_it_came_from() {
        let feeds = super::super::feeds::parse("name,input\nA,udp://@239.1.1.1:5000\nB,udp://@239.1.1.2:5000\n").unwrap();
        let answer = json!({
            "added": ["a"],
            "refused": [{ "index": 1, "name": "B", "why": "the port is taken by show a", "data": {} }],
            "plan": { "cost": { "cpu_millicores": 1250, "memory_mib": 300, "egress_kbps": 9000 }, "fits": true }
        });
        let text = outcome(&answer, &feeds, true);
        assert!(text.starts_with("would add 1 of 2 shows"), "{text}");
        assert!(text.contains("line 3 B: the port is taken"), "{text}");
        assert!(text.contains("cost 1.2 cores, 300 MiB, 9000 kbps out, fits"), "{text}");
    }
}
