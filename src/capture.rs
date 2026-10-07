//! Packet capture straight from the kernel, without libpcap.
//!
//! An `AF_PACKET` socket in cooked mode (`SOCK_DGRAM`) hands us every IP packet
//! on every interface with the link header already stripped, so Ethernet,
//! Wi-Fi, loopback and VPN tunnels (tun/wireguard) all look the same. It needs
//! `CAP_NET_RAW`, which `setcap cap_net_raw=ep` on the binary grants without
//! running the meter as root.

use std::collections::HashMap;
use std::ffi::CStr;
use std::io;
use std::sync::mpsc::SyncSender;
use std::time::{SystemTime, UNIX_EPOCH};

use a2tools_dps_meter_lib::capture::captured_payload::CapturedPayload;

const ETH_P_ALL: u16 = 0x0003;
const ETH_P_IP: u16 = 0x0800;
const ETH_P_IPV6: u16 = 0x86DD;
const PACKET_OUTGOING: u8 = 4;
const IPPROTO_TCP: u8 = 6;
/// GRO can glue segments into one receive of up to 64 KiB, so the buffer must
/// hold a whole one.
const RECV_BUFFER: usize = 256 * 1024;

/// Whether this process may open a raw socket (CAP_NET_RAW in its effective set).
pub fn has_capture_permission() -> bool {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return false;
    };
    const CAP_NET_RAW: u32 = 13;
    status
        .lines()
        .find_map(|line| line.strip_prefix("CapEff:"))
        .and_then(|hex| u64::from_str_radix(hex.trim(), 16).ok())
        .is_some_and(|caps| caps & (1 << CAP_NET_RAW) != 0)
}

struct RawSocket(libc::c_int);

impl Drop for RawSocket {
    fn drop(&mut self) {
        unsafe { libc::close(self.0) };
    }
}

fn open_socket() -> io::Result<RawSocket> {
    let fd = unsafe {
        libc::socket(
            libc::AF_PACKET,
            libc::SOCK_DGRAM,
            ETH_P_ALL.to_be() as libc::c_int,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let sock = RawSocket(fd);
    // Periodically return to the shutdown check even on a quiet network.
    let timeout = libc::timeval {
        tv_sec: 0,
        tv_usec: 250_000,
    };
    if unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            &timeout as *const _ as *const libc::c_void,
            std::mem::size_of::<libc::timeval>() as libc::socklen_t,
        )
    } < 0
    {
        return Err(io::Error::last_os_error());
    }
    // A big kernel buffer, so a burst during a boss fight is not dropped while
    // the parser catches up.
    let size: libc::c_int = 16 * 1024 * 1024;
    unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_RCVBUF,
            &size as *const _ as *const libc::c_void,
            std::mem::size_of::<libc::c_int>() as libc::socklen_t,
        );
    }
    Ok(sock)
}

fn interface_name(index: i32, cache: &mut HashMap<i32, String>) -> String {
    cache
        .entry(index)
        .or_insert_with(|| {
            let mut buf = [0 as libc::c_char; libc::IF_NAMESIZE];
            let ptr = unsafe { libc::if_indextoname(index as libc::c_uint, buf.as_mut_ptr()) };
            if ptr.is_null() {
                format!("if{index}")
            } else {
                unsafe { CStr::from_ptr(buf.as_ptr()) }
                    .to_string_lossy()
                    .into_owned()
            }
        })
        .clone()
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Capture until the receiver goes away. Blocks the calling thread.
pub fn run(tx: SyncSender<CapturedPayload>, stopping: impl Fn() -> bool) -> io::Result<()> {
    let sock = open_socket()?;
    let mut buf = vec![0u8; RECV_BUFFER];
    let mut names = HashMap::new();
    tracing::info!("Capture active on all interfaces");

    while !stopping() {
        let mut addr: libc::sockaddr_ll = unsafe { std::mem::zeroed() };
        let mut addr_len = std::mem::size_of::<libc::sockaddr_ll>() as libc::socklen_t;
        let n = unsafe {
            libc::recvfrom(
                sock.0,
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len(),
                0,
                &mut addr as *mut _ as *mut libc::sockaddr,
                &mut addr_len,
            )
        };
        if n < 0 {
            let err = io::Error::last_os_error();
            if matches!(
                err.kind(),
                io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
            ) {
                continue;
            }
            return Err(err);
        }
        let device = interface_name(addr.sll_ifindex, &mut names);
        // Loopback shows every packet twice, once leaving and once arriving.
        if addr.sll_pkttype == PACKET_OUTGOING && device == "lo" {
            continue;
        }
        let protocol = u16::from_be(addr.sll_protocol);
        let Some(payload) = parse_ip(protocol, &buf[..n as usize], device) else {
            continue;
        };
        if tx.send(payload).is_err() {
            return Ok(());
        }
    }
    Ok(())
}

/// The TCP payload of an IPv4 or IPv6 packet, or `None` for anything else.
pub fn parse_ip(protocol: u16, packet: &[u8], device: String) -> Option<CapturedPayload> {
    let (src_ip, dst_ip, tcp) = match protocol {
        ETH_P_IP => {
            if packet.len() < 20 || packet[0] >> 4 != 4 || packet[9] != IPPROTO_TCP {
                return None;
            }
            let ihl = (packet[0] & 0x0F) as usize * 4;
            let total = u16::from_be_bytes([packet[2], packet[3]]) as usize;
            // Fragmented IP datagrams require reassembly. Never interpret a
            // fragment's bytes as a complete TCP segment.
            let fragment = u16::from_be_bytes([packet[6], packet[7]]);
            if fragment & 0x3FFF != 0 {
                return None;
            }
            // Offloaded packets can report 0 as their total length.
            let end = if total == 0 { packet.len() } else { total };
            if end > packet.len() {
                return None;
            }
            if ihl < 20 || end < ihl {
                return None;
            }
            let src = std::net::Ipv4Addr::new(packet[12], packet[13], packet[14], packet[15]);
            let dst = std::net::Ipv4Addr::new(packet[16], packet[17], packet[18], packet[19]);
            (src.to_string(), dst.to_string(), &packet[ihl..end])
        }
        ETH_P_IPV6 => {
            // Extension headers are not followed; the game does not use them.
            if packet.len() < 40 || packet[0] >> 4 != 6 || packet[6] != IPPROTO_TCP {
                return None;
            }
            let plen = u16::from_be_bytes([packet[4], packet[5]]) as usize;
            let end = 40 + plen;
            if end > packet.len() {
                return None;
            }
            let src: [u8; 16] = packet[8..24].try_into().ok()?;
            let dst: [u8; 16] = packet[24..40].try_into().ok()?;
            (
                std::net::Ipv6Addr::from(src).to_string(),
                std::net::Ipv6Addr::from(dst).to_string(),
                &packet[40..end],
            )
        }
        _ => return None,
    };
    if tcp.len() < 20 {
        return None;
    }
    let data_offset = (tcp[12] >> 4) as usize * 4;
    if data_offset < 20 || data_offset >= tcp.len() {
        return None; // no payload: a bare ACK and the like
    }
    Some(CapturedPayload {
        src_port: u16::from_be_bytes([tcp[0], tcp[1]]),
        dst_port: u16::from_be_bytes([tcp[2], tcp[3]]),
        tcp_seq: u32::from_be_bytes([tcp[4], tcp[5], tcp[6], tcp[7]]),
        tcp_ack: u32::from_be_bytes([tcp[8], tcp[9], tcp[10], tcp[11]]),
        data: tcp[data_offset..].to_vec(),
        device_name: Some(device),
        captured_at_ms: now_ms(),
        src_ip: Some(src_ip),
        dst_ip: Some(dst_ip),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ipv4_tcp(payload: &[u8]) -> Vec<u8> {
        let mut tcp = vec![0u8; 20];
        tcp[0..2].copy_from_slice(&13328u16.to_be_bytes());
        tcp[2..4].copy_from_slice(&50000u16.to_be_bytes());
        tcp[4..8].copy_from_slice(&7u32.to_be_bytes());
        tcp[12] = 5 << 4;
        tcp.extend_from_slice(payload);
        let mut ip = vec![0u8; 20];
        ip[0] = 0x45;
        ip[2..4].copy_from_slice(&((20 + tcp.len()) as u16).to_be_bytes());
        ip[9] = IPPROTO_TCP;
        ip[12..16].copy_from_slice(&[10, 0, 0, 1]);
        ip[16..20].copy_from_slice(&[192, 168, 1, 2]);
        ip.extend(tcp);
        ip
    }

    #[test]
    fn reads_an_ipv4_tcp_payload() {
        let p = parse_ip(ETH_P_IP, &ipv4_tcp(&[1, 2, 3]), "eth0".into()).unwrap();
        assert_eq!(p.src_port, 13328);
        assert_eq!(p.dst_port, 50000);
        assert_eq!(p.tcp_seq, 7);
        assert_eq!(p.data, vec![1, 2, 3]);
        assert_eq!(p.src_ip.as_deref(), Some("10.0.0.1"));
    }

    #[test]
    fn ignores_empty_segments_and_other_protocols() {
        assert!(parse_ip(ETH_P_IP, &ipv4_tcp(&[]), "eth0".into()).is_none());
        let mut udp = ipv4_tcp(&[1]);
        udp[9] = 17;
        assert!(parse_ip(ETH_P_IP, &udp, "eth0".into()).is_none());
        assert!(parse_ip(0x0806, &[0; 40], "eth0".into()).is_none());
    }

    #[test]
    fn padding_past_the_ip_length_is_not_payload() {
        let mut p = ipv4_tcp(&[9, 9]);
        p.extend([0, 0, 0, 0]); // Ethernet minimum-frame padding
        let got = parse_ip(ETH_P_IP, &p, "eth0".into()).unwrap();
        assert_eq!(got.data, vec![9, 9]);
    }

    #[test]
    fn rejects_fragments_and_truncated_datagrams() {
        for fragment in [0x2000u16, 1, 0x2001] {
            let mut packet = ipv4_tcp(&[1, 2, 3]);
            packet[6..8].copy_from_slice(&fragment.to_be_bytes());
            assert!(parse_ip(ETH_P_IP, &packet, "eth0".into()).is_none());
        }
        let mut packet = ipv4_tcp(&[1, 2, 3]);
        packet.pop();
        assert!(parse_ip(ETH_P_IP, &packet, "eth0".into()).is_none());
        packet[2..4].copy_from_slice(&10u16.to_be_bytes());
        assert!(parse_ip(ETH_P_IP, &packet, "eth0".into()).is_none());
    }

    #[test]
    fn accepts_zero_length_offloaded_ipv4_and_ipv6_tcp() {
        let mut packet = ipv4_tcp(&[1, 2, 3]);
        packet[2..4].fill(0);
        assert_eq!(
            parse_ip(ETH_P_IP, &packet, "eth0".into()).unwrap().data,
            [1, 2, 3]
        );
        let mut ipv6 = vec![0u8; 40];
        ipv6[0] = 0x60;
        ipv6[4..6].copy_from_slice(&23u16.to_be_bytes());
        ipv6[6] = IPPROTO_TCP;
        ipv6.extend_from_slice(&packet[20..]);
        assert_eq!(
            parse_ip(ETH_P_IPV6, &ipv6, "eth0".into()).unwrap().data,
            [1, 2, 3]
        );
        ipv6.pop();
        assert!(parse_ip(ETH_P_IPV6, &ipv6, "eth0".into()).is_none());
    }
}
