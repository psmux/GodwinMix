//! The feeds list: `name,input,program,output,format`, a header first.

pub struct Row {
    pub name: String,
    pub input: String,
    pub program: u16,
    pub output: String,
    pub format: String,
}

pub fn read(path: &str) -> Result<Vec<Row>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("could not read {path}: {e}"))?;
    parse(&text).map_err(|e| format!("{path}: {e}"))
}

pub fn parse(text: &str) -> Result<Vec<Row>, String> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#'));
    let header: Vec<String> = lines.next().ok_or("the list is empty")?.split(',').map(|h| h.trim().to_lowercase()).collect();
    let col = |name: &str| header.iter().position(|h| h == name);
    let (name, input) = (col("name").ok_or("there is no name column")?, col("input").ok_or("there is no input column")?);
    let (program, output, format) = (col("program"), col("output"), col("format"));
    lines
        .enumerate()
        .map(|(i, l)| {
            let f: Vec<&str> = l.split(',').map(|v| v.trim().trim_matches('"')).collect();
            let get = |c: Option<usize>| c.and_then(|c| f.get(c)).copied().unwrap_or("");
            let program = match get(program) {
                "" => 0,
                p => p.parse().map_err(|_| format!("line {}: the program {p} is not a number", i + 2))?,
            };
            Ok(Row { name: get(Some(name)).into(), input: get(Some(input)).into(), program, output: get(output).into(), format: get(format).into() })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_list_the_feeds_command_writes() {
        let rows = parse("name,input,program,output,format\nfeed-001,udp://@239.77.0.1:5000,2,udp://127.0.0.1:30000,copy\n\n# a note\nb,udp://@:20001,,,\n").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].program, rows[0].format.as_str()), (2, "copy"));
        assert_eq!((rows[1].program, rows[1].output.as_str()), (0, ""));
        assert!(parse("input\nx").is_err());
        assert!(parse("name,input,program\na,b,two").is_err());
    }
}
