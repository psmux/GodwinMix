//! `.3dl`: the Lustre, Nuke and Flame form.
//!
//! The first line of numbers is the input mesh, the points each axis is cut
//! at (`0 64 128 ... 1023`), and its count is the cube's size. Every line
//! after it is one point of the cube, as whole numbers whose range the
//! largest value decides: 10, 12 or 16 bit. Blue changes fastest, the
//! opposite of `.cube`, so the rows are turned round as they are read.

use super::{Cube, Lut, MAX_SIZE};
use anyhow::{bail, Context, Result};

pub fn parse(text: &str) -> Result<Lut> {
    let mut mesh: Option<usize> = None;
    let mut rows: Vec<[f32; 3]> = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() || line.starts_with(|c: char| c.is_ascii_alphabetic()) {
            continue;
        }
        let nums: Vec<f32> = line
            .split_whitespace()
            .map(str::parse)
            .collect::<Result<_, _>>()
            .with_context(|| format!("line {}: {raw:?} is not a row of numbers", n + 1))?;
        match (mesh, nums.len()) {
            (None, k) if k >= 2 => mesh = Some(k),
            (Some(_), 3) => rows.push([nums[0], nums[1], nums[2]]),
            _ => bail!("line {}: {raw:?} has {} numbers where a .3dl row has 3", n + 1, nums.len()),
        }
    }
    let Some(size) = mesh else { bail!("there is no input mesh line, the first line of numbers a .3dl starts with") };
    if !(2..=MAX_SIZE).contains(&size) {
        bail!("a mesh of {size} points is outside 2 to {MAX_SIZE}");
    }
    if rows.len() != size * size * size {
        bail!("a mesh of {size} points needs {} rows and the file has {}", size * size * size, rows.len());
    }
    let top = rows.iter().flat_map(|r| r.iter()).fold(0.0f32, |a, &b| a.max(b));
    let scale = [1023.0, 4095.0, 65535.0].into_iter().find(|&s| top <= s).unwrap_or(top.max(1.0));
    let mut values = vec![[0.0; 3]; rows.len()];
    for (i, row) in rows.iter().enumerate() {
        let (r, g, b) = (i / (size * size), (i / size) % size, i % size);
        values[r + size * (g + size * b)] = row.map(|v| v / scale);
    }
    Ok(Lut { title: String::new(), shaper: None, cube: Some(Cube { size, min: [0.0; 3], max: [1.0; 3], values }) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blue_fastest_rows_land_in_the_right_places() {
        let mut text = String::from("0 1023\n");
        for r in 0..2 {
            for g in 0..2 {
                for b in 0..2 {
                    text.push_str(&format!("{} {} {}\n", r * 1023, g * 1023, b * 1023));
                }
            }
        }
        let lut = parse(&text).unwrap();
        assert_eq!(lut.sample([1.0, 0.0, 0.0]), [1.0, 0.0, 0.0]);
        assert_eq!(lut.sample([0.0, 0.0, 1.0]), [0.0, 0.0, 1.0]);
    }

    #[test]
    fn a_twelve_bit_file_is_scaled_by_its_range() {
        let mut text = String::from("0 4095\n");
        for _ in 0..8 {
            text.push_str("2048 2048 2048\n");
        }
        let out = parse(&text).unwrap().sample([0.2, 0.2, 0.2]);
        assert!((out[0] - 0.5).abs() < 0.01, "{out:?}");
    }

    #[test]
    fn a_missing_row_is_counted() {
        let err = parse("0 1023\n0 0 0\n").unwrap_err().to_string();
        assert!(err.contains("needs 8 rows"), "{err}");
    }
}
