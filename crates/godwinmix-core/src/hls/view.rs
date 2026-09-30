//! What the server reads of a ring without touching its bytes: how far it
//! has got, and the numbers a playlist is written from.

/// How far a rung has got, which is what an LL-HLS blocking reload waits on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Position {
    /// The newest segment that is whole.
    pub complete: Option<u64>,
    /// The segment being written and how many parts it has so far.
    pub open: Option<(u64, u32)>,
}

impl Position {
    /// Whether a playlist now carries segment `msn`, or part `part` of it when
    /// one is named: the condition `_HLS_msn` and `_HLS_part` wait for.
    pub fn reached(&self, msn: u64, part: Option<u32>) -> bool {
        if self.complete.is_some_and(|c| c >= msn) {
            return true;
        }
        match (part, self.open) {
            (Some(p), Some((open, parts))) => open > msn || (open == msn && parts > p),
            _ => false,
        }
    }

    /// The newest media sequence number a playlist lists, whole or not.
    pub fn newest(&self) -> Option<u64> {
        self.open.map(|(m, _)| m).or(self.complete)
    }
}

/// What a playlist is written from: every number, none of the bytes.
#[derive(Debug, Clone, Default)]
pub struct View {
    pub segments: Vec<SegmentView>,
    /// The longest whole segment this ring has ever held, which only grows,
    /// so `EXT-X-TARGETDURATION` never shrinks under a player.
    pub longest_ns: u64,
}

#[derive(Debug, Clone)]
pub struct SegmentView {
    pub msn: u64,
    pub pdt_ms: i64,
    pub init: u32,
    pub decode_time: Option<u64>,
    pub complete: bool,
    pub duration_ns: u64,
    pub bytes: usize,
    /// Duration and independence of each part.
    pub parts: Vec<(u64, bool)>,
}
