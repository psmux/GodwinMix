//! `--name value` options, a `--flag` alone meaning true, and repeats kept.

use std::collections::HashMap;
use std::str::FromStr;

pub struct Args {
    named: HashMap<String, Vec<String>>,
}

impl Args {
    pub fn parse(raw: Vec<String>) -> Result<Args, String> {
        let mut named: HashMap<String, Vec<String>> = HashMap::new();
        let mut it = raw.into_iter().peekable();
        while let Some(arg) = it.next() {
            let Some(name) = arg.strip_prefix("--") else {
                return Err(format!("{arg} is not an option. Options start with --, as in --count 200"));
            };
            let value = match it.peek() {
                Some(next) if !next.starts_with("--") => it.next().unwrap_or_default(),
                _ => "true".to_string(),
            };
            named.entry(name.to_string()).or_default().push(value);
        }
        Ok(Args { named })
    }

    /// Prints `text` and answers true when `--help` was given.
    pub fn help(&self, text: &str) -> bool {
        if self.flag("help") {
            println!("{}", text.trim());
        }
        self.flag("help")
    }

    pub fn flag(&self, name: &str) -> bool {
        self.str(name).is_some_and(|v| v != "false")
    }

    pub fn str(&self, name: &str) -> Option<&str> {
        self.named.get(name).and_then(|v| v.last()).map(String::as_str)
    }

    pub fn all(&self, name: &str) -> Vec<&str> {
        self.named.get(name).map(|v| v.iter().map(String::as_str).collect()).unwrap_or_default()
    }

    pub fn need(&self, name: &str) -> Result<&str, String> {
        self.str(name).ok_or(format!("--{name} is needed. --help lists every option"))
    }

    pub fn num<T: FromStr>(&self, name: &str, default: T) -> Result<T, String> {
        match self.str(name) {
            None => Ok(default),
            Some(v) => v.parse().map_err(|_| format!("--{name} takes a number, not {v}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn of(s: &str) -> Args {
        Args::parse(s.split_whitespace().map(String::from).collect()).unwrap()
    }

    #[test]
    fn values_flags_and_repeats() {
        let a = of("--count 200 --unicast --impair 0-9:loss=1 --impair 20:jitter=5");
        assert_eq!(a.num("count", 0u32).unwrap(), 200);
        assert!(a.flag("unicast"));
        assert_eq!(a.all("impair"), vec!["0-9:loss=1", "20:jitter=5"]);
        assert!(a.num::<u32>("missing", 7).unwrap() == 7);
        assert!(Args::parse(vec!["loose".into()]).is_err());
    }
}
