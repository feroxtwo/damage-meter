//! Zone loads, for telling when an expedition was entered again.

/// The map a zone load names, from a packet's payload (opcode first):
/// `21 36 <u32 count> <u32 map id> ...`. Layout from A2Tools DPS Meter's
/// `parse_map_load_packet` (GPL-3.0), which keeps it to itself.
pub fn map_load(payload: &[u8]) -> Option<i32> {
    if payload.len() < 10 || payload[0] != 0x21 || payload[1] != 0x36 {
        return None;
    }
    Some(i32::from_le_bytes(payload[6..10].try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_map_of_a_zone_load_only() {
        let mut p = vec![0x21, 0x36, 1, 0, 0, 0];
        p.extend_from_slice(&600_011_i32.to_le_bytes());
        p.extend_from_slice(&[0; 8]);
        assert_eq!(map_load(&p), Some(600_011));
        assert_eq!(map_load(&p[..9]), None);
        p[0] = 0x23;
        assert_eq!(map_load(&p), None);
    }
}
