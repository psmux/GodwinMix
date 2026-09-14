//! A four function calculator for layout bindings.
//!
//! A layout preset has to say "the inset is `inset` times the canvas width,
//! `gap` in from the right edge", and it has to say it in a data file that a
//! person or an agent edits without touching Rust. That is one line of
//! arithmetic over the layout's own params, so this reads exactly that: numbers,
//! names, `+ - * /`, brackets, and a leading minus. No functions, no
//! comparisons, no calls, nothing that can loop.

use std::collections::BTreeMap;

/// Values a binding may name: the layout's numeric params plus `W` and `H`, the
/// canvas width and height in pixels.
pub type Scope = BTreeMap<String, f64>;

/// Evaluate an expression against a scope.
pub fn eval(source: &str, scope: &Scope) -> Result<f64, String> {
    let mut p = Parser {
        text: source,
        rest: source.trim(),
        scope,
    };
    let value = p.expr()?;
    if !p.rest.is_empty() {
        return Err(p.fail(&format!("unexpected {:?}", p.rest)));
    }
    if !value.is_finite() {
        return Err(p.fail("the result is not a finite number (a division by zero, most likely)"));
    }
    Ok(value)
}

struct Parser<'a> {
    text: &'a str,
    rest: &'a str,
    scope: &'a Scope,
}

impl Parser<'_> {
    /// `term (('+' | '-') term)*`
    fn expr(&mut self) -> Result<f64, String> {
        let mut value = self.term()?;
        while let Some(op) = self.take_one(&['+', '-']) {
            let rhs = self.term()?;
            value = if op == '+' { value + rhs } else { value - rhs };
        }
        Ok(value)
    }

    /// `factor (('*' | '/') factor)*`
    fn term(&mut self) -> Result<f64, String> {
        let mut value = self.factor()?;
        while let Some(op) = self.take_one(&['*', '/']) {
            let rhs = self.factor()?;
            value = if op == '*' { value * rhs } else { value / rhs };
        }
        Ok(value)
    }

    /// `number | name | '(' expr ')' | '-' factor`
    fn factor(&mut self) -> Result<f64, String> {
        self.skip_spaces();
        if self.take_one(&['-']).is_some() {
            return Ok(-self.factor()?);
        }
        if self.take_one(&['+']).is_some() {
            return self.factor();
        }
        if self.take_one(&['(']).is_some() {
            let value = self.expr()?;
            self.skip_spaces();
            if self.take_one(&[')']).is_none() {
                return Err(self.fail("a bracket was opened and never closed"));
            }
            return Ok(value);
        }
        let number = self.rest.len()
            - self
                .rest
                .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.')
                .len();
        if number > 0 {
            let (text, rest) = self.rest.split_at(number);
            self.rest = rest;
            return text
                .parse::<f64>()
                .map_err(|_| self.fail(&format!("{text:?} is not a number")));
        }
        let name = self.rest.len()
            - self
                .rest
                .trim_start_matches(|c: char| c.is_alphanumeric() || c == '_')
                .len();
        if name > 0 {
            let (text, rest) = self.rest.split_at(name);
            self.rest = rest;
            return self.scope.get(text).copied().ok_or_else(|| {
                let known: Vec<&str> = self.scope.keys().map(String::as_str).collect();
                self.fail(&format!(
                    "{text:?} is not a parameter here. Known names: {}",
                    known.join(", ")
                ))
            });
        }
        Err(self.fail("expected a number, a parameter name or a bracket"))
    }

    /// Take the next character when it is one of `wanted`.
    fn take_one(&mut self, wanted: &[char]) -> Option<char> {
        self.skip_spaces();
        let c = self.rest.chars().next()?;
        if wanted.contains(&c) {
            self.rest = &self.rest[c.len_utf8()..];
            Some(c)
        } else {
            None
        }
    }

    fn skip_spaces(&mut self) {
        self.rest = self.rest.trim_start();
    }

    /// An error that shows the whole expression, because one line of context is
    /// worth more than a column number.
    fn fail(&self, why: &str) -> String {
        format!("cannot read the binding {:?}: {why}", self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> Scope {
        Scope::from([
            ("W".into(), 1920.0),
            ("H".into(), 1080.0),
            ("inset".into(), 0.28),
            ("gap".into(), 0.02),
        ])
    }

    #[test]
    fn the_arithmetic_a_layout_actually_uses() {
        let s = scope();
        for (text, want) in [
            ("W", 1920.0),
            ("inset*W", 537.6),
            ("W - inset*W - gap*W", 1344.0),
            ("(W - gap*W)/2", 940.8),
            ("2 * (1 + 2)", 6.0),
            ("-gap*W", -38.4),
            ("H/2", 540.0),
            ("0.5", 0.5),
        ] {
            let got = eval(text, &s).unwrap_or_else(|e| panic!("{text}: {e}"));
            assert!(
                (got - want).abs() < 1e-9,
                "{text} gave {got}, wanted {want}"
            );
        }
    }

    #[test]
    fn multiplication_binds_tighter_than_addition() {
        assert_eq!(eval("1 + 2 * 3", &scope()).unwrap(), 7.0);
        assert_eq!(eval("(1 + 2) * 3", &scope()).unwrap(), 9.0);
    }

    #[test]
    fn an_unknown_name_lists_the_names_that_do_exist() {
        let err = eval("wobble * W", &scope()).unwrap_err();
        assert!(err.contains("not a parameter here"), "{err}");
        assert!(err.contains("inset"), "{err}");
    }

    #[test]
    fn broken_expressions_say_what_is_wrong_and_show_the_line() {
        for (text, needle) in [
            ("(W", "never closed"),
            ("W W", "unexpected"),
            ("* W", "expected a number"),
            ("W/0", "not a finite number"),
        ] {
            let err = eval(text, &scope()).unwrap_err();
            assert!(err.contains(needle), "{text}: {err}");
            assert!(err.contains(text), "{text}: {err}");
        }
    }
}
