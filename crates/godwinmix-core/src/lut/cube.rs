//! `.cube`: the form Resolve and Adobe write, and the one most camera makers
//! publish their LUTs in.
//!
//! ```text
//! TITLE "Warm"
//! DOMAIN_MIN 0 0 0
//! DOMAIN_MAX 1 1 1
//! LUT_1D_SIZE 1024        a shaper, optional
//! LUT_3D_SIZE 33          the cube, optional
//! 0.0 0.0 0.0             the shaper's rows, then the cube's, red fastest
//! ```
//!
//! A file with both sizes is Resolve's form: the 1D rows come first and run
//! before the cube. `LUT_1D_INPUT_RANGE` and `LUT_3D_INPUT_RANGE` are
//! Resolve's names for a domain and are read the same way.

use super::{Cube, Lut, Shaper, MAX_SHAPER, MAX_SIZE};
use anyhow::{bail, Context, Result};

#[derive(Default)]
struct Header {
    title: String,
    size_1d: Option<usize>,
    size_3d: Option<usize>,
    min_1d: Option<[f32; 3]>,
    max_1d: Option<[f32; 3]>,
    min_3d: Option<[f32; 3]>,
    max_3d: Option<[f32; 3]>,
}

pub fn parse(text: &str) -> Result<Lut> {
    let mut h = Header::default();
    let mut rows: Vec<[f32; 3]> = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let at = || format!("line {}: {raw:?}", n + 1);
        let first = line.as_bytes()[0];
        if first.is_ascii_digit() || first == b'-' || first == b'.' || first == b'+' {
            rows.push(three(line).with_context(at)?);
        } else {
            keyword(&mut h, line).with_context(at)?;
        }
    }
    build(h, rows)
}

fn keyword(h: &mut Header, line: &str) -> Result<()> {
    let (key, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let rest = rest.trim();
    match key.to_ascii_uppercase().as_str() {
        "TITLE" => h.title = rest.trim_matches('"').to_string(),
        "LUT_1D_SIZE" => h.size_1d = Some(size(rest, 2, MAX_SHAPER)?),
        "LUT_3D_SIZE" => h.size_3d = Some(size(rest, 2, MAX_SIZE)?),
        "DOMAIN_MIN" => (h.min_1d, h.min_3d) = (Some(three(rest)?), Some(three(rest)?)),
        "DOMAIN_MAX" => (h.max_1d, h.max_3d) = (Some(three(rest)?), Some(three(rest)?)),
        "LUT_1D_INPUT_RANGE" => (h.min_1d, h.max_1d) = range(rest)?,
        "LUT_3D_INPUT_RANGE" => (h.min_3d, h.max_3d) = range(rest)?,
        // Written by some tools and harmless to skip.
        "LUT_IN_VIDEO_RANGE" | "LUT_OUT_VIDEO_RANGE" => {}
        other => bail!("{other} is not a .cube keyword. A .cube has TITLE, LUT_1D_SIZE, LUT_3D_SIZE, DOMAIN_MIN, DOMAIN_MAX and rows of three numbers"),
    }
    Ok(())
}

fn size(text: &str, lo: usize, hi: usize) -> Result<usize> {
    let n: usize = text.parse().with_context(|| format!("{text:?} is not a whole number"))?;
    if !(lo..=hi).contains(&n) {
        bail!("a size of {n} is outside {lo} to {hi}");
    }
    Ok(n)
}

/// A domain's low and high, each for all three channels.
type Domain = (Option<[f32; 3]>, Option<[f32; 3]>);

fn range(text: &str) -> Result<Domain> {
    let v: Vec<f32> = text.split_whitespace().map(str::parse).collect::<Result<_, _>>().context("a range is two numbers")?;
    let [lo, hi] = v[..] else { bail!("a range is two numbers, the low and the high") };
    Ok((Some([lo; 3]), Some([hi; 3])))
}

fn three(text: &str) -> Result<[f32; 3]> {
    let mut it = text.split_whitespace().map(str::parse::<f32>);
    let (Some(Ok(a)), Some(Ok(b)), Some(Ok(c)), None) = (it.next(), it.next(), it.next(), it.next()) else {
        bail!("expected three numbers");
    };
    if !(a.is_finite() && b.is_finite() && c.is_finite()) {
        bail!("a value is not a finite number");
    }
    Ok([a, b, c])
}

fn build(h: Header, rows: Vec<[f32; 3]>) -> Result<Lut> {
    let n1 = h.size_1d.unwrap_or(0);
    let n3 = h.size_3d.map(|n| n * n * n).unwrap_or(0);
    if n1 == 0 && n3 == 0 {
        bail!("there is no LUT_1D_SIZE or LUT_3D_SIZE line, so the size of the table is unknown");
    }
    if rows.len() != n1 + n3 {
        bail!("the header asks for {} rows and the file has {}", n1 + n3, rows.len());
    }
    let (one, three) = rows.split_at(n1);
    let shaper = (n1 > 0).then(|| Shaper { min: h.min_1d.unwrap_or([0.0; 3]), max: h.max_1d.unwrap_or([1.0; 3]), values: one.to_vec() });
    let cube = h.size_3d.map(|size| Cube { size, min: h.min_3d.unwrap_or([0.0; 3]), max: h.max_3d.unwrap_or([1.0; 3]), values: three.to_vec() });
    for (min, max) in [shaper.as_ref().map(|s| (s.min, s.max)), cube.as_ref().map(|c| (c.min, c.max))].into_iter().flatten() {
        if (0..3).any(|i| max[i] <= min[i]) {
            bail!("the domain's maximum {max:?} is not above its minimum {min:?}");
        }
    }
    Ok(Lut { title: h.title, shaper, cube })
}

/// The `.cube` text for `lut`, which `parse` reads back to the same table.
pub fn write(lut: &Lut) -> String {
    let mut out = String::new();
    let row = |out: &mut String, v: &[f32; 3]| out.push_str(&format!("{:.6} {:.6} {:.6}\n", v[0], v[1], v[2]));
    if !lut.title.is_empty() {
        out.push_str(&format!("TITLE \"{}\"\n", lut.title.replace('"', "'")));
    }
    if let Some(s) = &lut.shaper {
        out.push_str(&format!("LUT_1D_SIZE {}\n", s.values.len()));
        out.push_str(&format!("LUT_1D_INPUT_RANGE {} {}\n", s.min[0], s.max[0]));
    }
    if let Some(c) = &lut.cube {
        out.push_str(&format!("LUT_3D_SIZE {}\n", c.size));
        out.push_str(&format!("LUT_3D_INPUT_RANGE {} {}\n", c.min[0], c.max[0]));
    }
    for v in lut.shaper.iter().flat_map(|s| s.values.iter()).chain(lut.cube.iter().flat_map(|c| c.values.iter())) {
        row(&mut out, v);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SMALL: &str = "# made by hand\nTITLE \"Swap\"\nLUT_3D_SIZE 2\n\
        0 0 0\n0 0 1\n0 1 0\n0 1 1\n1 0 0\n1 0 1\n1 1 0\n1 1 1\n";

    #[test]
    fn a_two_point_cube_reads_red_fastest() {
        let lut = parse(SMALL).unwrap();
        assert_eq!(lut.title, "Swap");
        // This one swaps red and blue: red in is blue out.
        assert_eq!(lut.sample([1.0, 0.0, 0.0]), [0.0, 0.0, 1.0]);
    }

    #[test]
    fn writing_and_reading_back_keeps_the_table() {
        let lut = super::super::Lut::from_fn("Warm", 5, |[r, g, b]| [r * 0.9, g, (b + 0.1).min(1.0)]);
        let back = parse(&write(&lut)).unwrap();
        assert_eq!(back.size(), 5);
        let (a, b) = (lut.sample([0.3, 0.6, 0.2]), back.sample([0.3, 0.6, 0.2]));
        assert!((0..3).all(|i| (a[i] - b[i]).abs() < 1e-5), "{a:?} {b:?}");
    }

    #[test]
    fn a_shaper_and_a_cube_both_run() {
        let text = "LUT_1D_SIZE 2\nLUT_3D_SIZE 2\n0 0 0\n0.5 0.5 0.5\n\
            0 0 0\n1 0 0\n0 1 0\n1 1 0\n0 0 1\n1 0 1\n0 1 1\n1 1 1\n";
        let lut = parse(text).unwrap();
        let out = lut.sample([1.0, 1.0, 1.0]);
        assert!((out[0] - 0.5).abs() < 1e-6, "{out:?}");
    }

    #[test]
    fn a_short_file_says_how_many_rows_it_wanted() {
        let err = parse("LUT_3D_SIZE 2\n0 0 0\n").unwrap_err().to_string();
        assert!(err.contains("asks for 8 rows and the file has 1"), "{err}");
    }

    #[test]
    fn a_bad_row_names_its_line() {
        let err = format!("{:#}", parse("LUT_3D_SIZE 2\n0 0\n").unwrap_err());
        assert!(err.contains("line 2"), "{err}");
    }

    #[test]
    fn a_huge_size_is_refused_before_anything_is_read() {
        assert!(parse("LUT_3D_SIZE 4096\n").is_err());
    }
}
