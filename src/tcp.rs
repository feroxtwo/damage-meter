//! Bounded TCP payload ordering. Capture may start mid-stream; absent bytes
//! cannot be reconstructed and a recovery is explicitly reported as a gap.
#[derive(Default)]
pub struct TcpOrder {
    next: Option<u32>,
    pending: Vec<(u32, Vec<u8>)>,
    gap_since: Option<i64>,
    pub gaps: u64,
    pub duplicates: u64,
}
impl TcpOrder {
    pub fn pending_bytes(&self) -> usize {
        self.pending.iter().map(|(_, d)| d.len()).sum()
    }
    pub fn feed(&mut self, seq: u32, data: &[u8], now: i64) -> (Vec<Vec<u8>>, bool) {
        if data.is_empty() {
            return (Vec::new(), false);
        }
        let mut next = *self.next.get_or_insert(seq);
        let end = seq.wrapping_add(data.len() as u32);
        if (end.wrapping_sub(next) as i32) <= 0 {
            self.duplicates += 1;
            return (Vec::new(), false);
        }
        self.pending.push((seq, data.to_vec()));
        let mut reset = false;
        let bytes: usize = self.pending.iter().map(|(_, d)| d.len()).sum();
        let fills_gap = self.pending.iter().any(|(s, d)| {
            s.wrapping_sub(next) as i32 <= 0
                && s.wrapping_add(d.len() as u32).wrapping_sub(next) as i32 > 0
        });
        if bytes > 2 << 20
            || self.pending.len() > 2048
            || (!fills_gap && self.gap_since.is_some_and(|t| now - t >= 2000))
        {
            self.pending.sort_by_key(|(s, _)| s.wrapping_sub(next));
            next = self.pending.first().unwrap().0;
            self.next = Some(next);
            self.gaps += 1;
            reset = true;
            self.gap_since = None;
        }
        let mut chunks = Vec::new();
        loop {
            let eligible = self
                .pending
                .iter()
                .enumerate()
                .filter(|(_, (s, d))| {
                    (*s).wrapping_sub(next) as i32 <= 0
                        && s.wrapping_add(d.len() as u32).wrapping_sub(next) as i32 > 0
                })
                .min_by_key(|(_, (s, _))| s.wrapping_sub(next) as i32)
                .map(|(i, _)| i);
            let Some(i) = eligible else { break };
            let (s, data) = self.pending.remove(i);
            let skip = next.wrapping_sub(s) as usize;
            let chunk = data[skip..].to_vec();
            next = next.wrapping_add(chunk.len() as u32);
            chunks.push(chunk);
        }
        self.pending
            .retain(|(s, d)| s.wrapping_add(d.len() as u32).wrapping_sub(next) as i32 > 0);
        self.next = Some(next);
        if self.pending.is_empty() {
            self.gap_since = None;
        } else {
            self.gap_since.get_or_insert(now);
        }
        (chunks, reset)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn orders_overlaps_duplicates_and_wraparound() {
        let mut t = TcpOrder::default();
        let start = u32::MAX - 2;
        assert_eq!(t.feed(start, b"abc", 0).0, vec![b"abc".to_vec()]);
        assert!(t.feed(3, b"ghi", 1).0.is_empty());
        assert_eq!(
            t.feed(0, b"def", 2).0,
            vec![b"def".to_vec(), b"ghi".to_vec()]
        );
        assert!(t.feed(0, b"def", 3).0.is_empty());
        assert_eq!(t.duplicates, 1);
        assert_eq!(t.feed(4, b"hijk", 4).0, vec![b"jk".to_vec()]);
    }
    #[test]
    fn missing_bytes_are_marked_instead_of_silently_appended() {
        let mut t = TcpOrder::default();
        t.feed(1, b"a", 0);
        assert!(t.feed(5, b"b", 1).0.is_empty());
        let (chunks, reset) = t.feed(6, b"c", 2002);
        assert!(reset);
        assert_eq!(chunks, vec![b"b".to_vec(), b"c".to_vec()]);
        assert_eq!(t.gaps, 1);
    }
    #[test]
    fn late_missing_segment_fills_gap_without_discarding_valid_framing() {
        let mut t = TcpOrder::default();
        t.feed(1, b"a", 0);
        t.feed(3, b"c", 1);
        let (chunks, reset) = t.feed(2, b"b", 3000);
        assert!(!reset);
        assert_eq!(chunks, vec![b"b".to_vec(), b"c".to_vec()]);
    }
}
