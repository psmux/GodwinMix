//! Comma separated values with a header row, the way a published Google
//! Sheet or an Excel export writes them: quoted fields, doubled quotes, line
//! breaks inside quotes, CRLF or LF. Read into
//!
//! ```json
//! { "columns": ["Name", "Title"], "rows": [ { "Name": "Ada", "Title": "Host" } ] }
//! ```
//!
//! so `rows[0].Name` is a cell and `rows[].Name` a column.

use serde_json::{json, Map, Value};

pub fn read(text: &str) -> Result<Value, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut records = records(text)?.into_iter().filter(|r| r.iter().any(|f| !f.trim().is_empty()));
    let Some(header) = records.next() else {
        return Ok(json!({ "columns": [], "rows": [] }));
    };
    let columns = names(&header);
    let rows: Vec<Value> = records
        .map(|record| {
            let mut row = Map::new();
            for (i, name) in columns.iter().enumerate() {
                row.insert(name.clone(), json!(record.get(i).map(|s| s.trim()).unwrap_or("")));
            }
            Value::Object(row)
        })
        .collect();
    Ok(json!({ "columns": columns, "rows": rows }))
}

/// Header names, trimmed, with a blank one called `column_3` and a repeated
/// one given `_2`, so every cell has a key.
fn names(header: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(header.len());
    for (i, raw) in header.iter().enumerate() {
        let base = match raw.trim() {
            "" => format!("column_{}", i + 1),
            name => name.to_string(),
        };
        let mut name = base.clone();
        let mut n = 2;
        while out.contains(&name) {
            name = format!("{base}_{n}");
            n += 1;
        }
        out.push(name);
    }
    out
}

fn records(text: &str) -> Result<Vec<Vec<String>>, String> {
    let mut out = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            (true, '"') => quoted = false,
            (true, c) => field.push(c),
            (false, '"') if field.is_empty() => quoted = true,
            (false, ',') => record.push(std::mem::take(&mut field)),
            (false, '\r') if chars.peek() == Some(&'\n') => {}
            (false, '\n') => {
                record.push(std::mem::take(&mut field));
                out.push(std::mem::take(&mut record));
            }
            (false, c) => field.push(c),
        }
    }
    if quoted {
        return Err("a quoted field is never closed, so the file was cut short or is not CSV".into());
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        out.push(record);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_published_sheet_reads_into_rows_by_column() {
        let text = "\u{feff}Name,Title,\r\n\"Lovelace, Ada\",\"Says \"\"hello\"\"\",x\r\n\r\nGrace Hopper,\"Rear\nAdmiral\"\r\n";
        let doc = read(text).unwrap();
        assert_eq!(doc["columns"], json!(["Name", "Title", "column_3"]));
        assert_eq!(doc["rows"][0], json!({ "Name": "Lovelace, Ada", "Title": "Says \"hello\"", "column_3": "x" }));
        assert_eq!(doc["rows"][1]["Title"], "Rear\nAdmiral");
        assert_eq!(doc["rows"][1]["column_3"], "");
        assert_eq!(doc["rows"].as_array().unwrap().len(), 2, "a blank line is not a row");
    }

    #[test]
    fn a_repeated_header_gets_a_number() {
        let doc = read("a,a,a\n1,2,3").unwrap();
        assert_eq!(doc["columns"], json!(["a", "a_2", "a_3"]));
    }

    #[test]
    fn an_open_quote_is_refused() {
        assert!(read("a\n\"b").unwrap_err().contains("never closed"));
    }
}
