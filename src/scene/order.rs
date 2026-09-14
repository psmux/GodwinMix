//! Fractional order keys for siblings in the flat record store.
//!
//! Two clients drag two different items in the same scene at the same time. If
//! order were an array index, each of them renumbers every sibling and the two
//! edits collide on rows neither of them touched. A fractional key is a string
//! that sorts between its neighbours, so an insert writes one record and only
//! that record (Excalidraw's and tldraw's fractional indexing, cited in 11
//! section 2).
//!
//! A key is the fractional part of a base 62 number, written in an alphabet
//! whose byte order is its digit order, so plain string comparison is numeric
//! comparison. The one rule that makes it work: a key never ends in the zero
//! digit, because `"1"` and `"10"` would otherwise be the same number and
//! there would be nothing between them.

/// Digits in byte order: `'0'..'9' < 'A'..'Z' < 'a'..'z'` in ASCII.
const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
/// A digit near the middle of the alphabet, used when a key would otherwise
/// end in zero and as the first key in an empty list.
const MID: u8 = DIGITS[31];

/// The order keys for `n` siblings, evenly spread so later inserts between any
/// two of them stay short.
pub fn spread(n: usize) -> Vec<String> {
    let mut width = 1usize;
    let mut span = 62u128;
    while span < (n as u128) + 1 {
        span *= 62;
        width += 1;
    }
    (0..n)
        .map(|i| {
            let v = ((i as u128) + 1) * span / ((n as u128) + 1);
            let mut s = base62(v, width);
            if s.ends_with('0') {
                s.push(MID as char);
            }
            s
        })
        .collect()
}

/// `v` in base 62, zero padded to `width` digits.
fn base62(mut v: u128, width: usize) -> String {
    let mut out = vec![b'0'; width];
    for slot in out.iter_mut().rev() {
        *slot = DIGITS[(v % 62) as usize];
        v /= 62;
    }
    String::from_utf8(out).expect("base 62 digits are ASCII")
}

/// A key strictly between `before` and `after`. `None` means the start or the
/// end of the list, so `between(None, None)` is the first key in a scene.
///
/// Returns `None` when the two keys are not in order, which is a caller bug and
/// is reported rather than papered over.
pub fn between(before: Option<&str>, after: Option<&str>) -> Option<String> {
    let a = before.unwrap_or("");
    if let Some(b) = after {
        if a >= b || b.is_empty() {
            return None;
        }
    }
    if a.ends_with('0') || after.is_some_and(|b| b.ends_with('0')) {
        return None;
    }
    Some(midpoint(a, after))
}

/// The midpoint of two fractional parts, `a` empty meaning zero and `b` absent
/// meaning one. Assumes `a < b` and neither ends in the zero digit.
fn midpoint(a: &str, b: Option<&str>) -> String {
    if let Some(b) = b {
        // Strip the shared prefix and recurse on what is left, so the answer
        // stays as short as the neighbours allow.
        let shared = a
            .bytes()
            .chain(std::iter::repeat(b'0'))
            .zip(b.bytes())
            .take_while(|(x, y)| x == y)
            .count();
        if shared > 0 {
            let tail = midpoint(a.get(shared..).unwrap_or(""), Some(&b[shared..]));
            return format!("{}{}", &b[..shared], tail);
        }
    }
    let digit_a = a.bytes().next().map_or(0, digit);
    let digit_b = b.map_or(DIGITS.len(), |b| b.bytes().next().map_or(0, digit));
    if digit_b - digit_a > 1 {
        return ((DIGITS[(digit_a + digit_b).div_ceil(2)]) as char).to_string();
    }
    match b {
        // The neighbours' first digits are next to each other, so the answer
        // starts inside `b`: its first digit alone already sits between them.
        Some(b) if b.len() > 1 => b[..1].to_string(),
        // Nothing to borrow from `b`, so keep `a`'s first digit and go deeper.
        _ => format!("{}{}", DIGITS[digit_a] as char, midpoint(a.get(1..).unwrap_or(""), None)),
    }
}

/// Index of a digit in the alphabet. A byte that is not a digit sorts as zero,
/// which only happens for a key this module did not write.
fn digit(b: u8) -> usize {
    DIGITS.iter().position(|d| *d == b).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spread_keys_are_ordered_distinct_and_never_end_in_zero() {
        for n in [0usize, 1, 2, 3, 16, 61, 62, 63, 200, 4000] {
            let keys = spread(n);
            assert_eq!(keys.len(), n);
            let mut sorted = keys.clone();
            sorted.sort();
            assert_eq!(keys, sorted, "spread({n}) came out unordered");
            sorted.dedup();
            assert_eq!(sorted.len(), n, "spread({n}) repeated a key");
            assert!(keys.iter().all(|k| !k.ends_with('0')), "spread({n}) ended a key in zero");
        }
    }

    #[test]
    fn inserting_between_two_keys_never_disturbs_either() {
        let mut keys = spread(4);
        // A thousand inserts into the same gap, the worst case a dragging
        // client can produce.
        for _ in 0..1000 {
            let next = between(Some(&keys[1]), Some(&keys[2])).expect("a key fits in the gap");
            assert!(keys[1] < next && next < keys[2], "{} < {next} < {}", keys[1], keys[2]);
            assert!(!next.ends_with('0'));
            keys.insert(2, next);
        }
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }

    #[test]
    fn the_ends_of_the_list_work_too() {
        let first = between(None, None).unwrap();
        let before = between(None, Some(&first)).unwrap();
        let after = between(Some(&first), None).unwrap();
        assert!(before < first && first < after);
        let mut tail = after;
        for _ in 0..200 {
            let next = between(Some(&tail), None).unwrap();
            assert!(next > tail);
            tail = next;
        }
    }

    #[test]
    fn keys_out_of_order_are_refused_rather_than_guessed_at() {
        assert_eq!(between(Some("V"), Some("V")), None);
        assert_eq!(between(Some("k"), Some("V")), None);
        assert_eq!(between(Some("V0"), None), None);
    }
}
