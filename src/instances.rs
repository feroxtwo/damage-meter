//! Zone loads and the group list: where you are and who you play with.

/// The map a zone load names, from a packet's payload (opcode first):
/// `21 36 <u32 count> <u32 map id> ...`. Layout from A2Tools DPS Meter's
/// `parse_map_load_packet` (GPL-3.0), which keeps it to itself.
pub fn map_load(payload: &[u8]) -> Option<i32> {
    if payload.len() < 10 || payload[0] != 0x21 || payload[1] != 0x36 {
        return None;
    }
    Some(i32::from_le_bytes(payload[6..10].try_into().ok()?))
}

/// The members of your group, from the game's group list (`00 92`). It
/// comes when the group changes and on every load into an instance, always
/// complete, you included. Not decoded by A2Tools, whose roster (`02 97`)
/// is the group finder's: a group formed in the open world never sends one.
///
/// Each member carries a 36-character id as a length-prefixed string (`24`),
/// then the character's u64 id, then the name, length-prefixed. Layout from
/// Mirco's 2026-10-07 recordings (open world and expedition groups).
pub fn group_list(payload: &[u8]) -> Option<Vec<String>> {
    if payload.len() < 2 || payload[0] != 0x00 || payload[1] != 0x92 {
        return None;
    }
    let mut names = Vec::new();
    let mut i = 2;
    while i + 1 + 36 + 8 + 1 < payload.len() {
        if payload[i] == 0x24 && is_uuid(&payload[i + 1..i + 37]) {
            let at = i + 37 + 8;
            let len = payload[at] as usize;
            if let Some(name) = payload
                .get(at + 1..at + 1 + len)
                .filter(|_| (1..=40).contains(&len))
                .and_then(|n| std::str::from_utf8(n).ok())
            {
                names.push(name.to_string());
                i = at + 1 + len;
                continue;
            }
        }
        i += 1;
    }
    Some(names)
}

fn is_uuid(s: &[u8]) -> bool {
    s.len() == 36
        && s.iter().enumerate().all(|(i, &c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_hexdigit()
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(name: &str) -> Vec<u8> {
        let mut m = vec![
            0x01, 0x00, 0x03, 0xa9, 0x1d, 0, 0, 0xfd, 0x08, 0xfd, 0x08, 0x24,
        ];
        m.extend_from_slice(b"03D7EFCF-F53C-4F08-915A-5B1E105E18E2");
        m.extend_from_slice(&[0xfb, 0xa7, 0x03, 0, 0, 0, 0xfd, 0x08, name.len() as u8]);
        m.extend_from_slice(name.as_bytes());
        m.extend_from_slice(&[0x0c, 0, 0, 0, 0xb6, 0x9b, 0x01]);
        m
    }

    #[test]
    fn reads_every_member_of_a_group_list() {
        let mut p = vec![0x00, 0x92, 0x08, 0xdc, 0x77, 0x05, 0x00, 0x08, 0x39];
        for name in ["Malondro", "Ferox", "NexHealer"] {
            p.extend(member(name));
            p.extend_from_slice(&[0; 40]);
        }
        assert_eq!(group_list(&p).unwrap(), ["Malondro", "Ferox", "NexHealer"]);
        assert_eq!(group_list(&[0x00, 0x92]).unwrap(), Vec::<String>::new());
        assert!(group_list(&[0x02, 0x97, 0, 0]).is_none());
    }

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
