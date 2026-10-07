//! Offline, deterministic decoder for v1, v2 and v3 meter captures.
use crate::{db::Db, dispatcher::EffectScanner, engine::Engine, tcp::TcpOrder};
use a2tools_dps_meter_lib::capture::{
    captured_payload::CapturedPayload, stream_assembler::StreamAssembler,
    stream_processor::StreamProcessor,
};
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufReader, Read};
use std::path::Path;
#[derive(Deserialize)]
struct PacketHeader {
    ms: i64,
    from_server: bool,
    src_port: u16,
    dst_port: u16,
    src_ip: Option<String>,
    dst_ip: Option<String>,
    device: Option<String>,
    seq: u32,
    ack: u32,
}
#[derive(Deserialize)]
struct JsonPacket {
    #[serde(flatten)]
    header: PacketHeader,
    // Required in v2. A missing payload is corruption, not an empty packet.
    data: Vec<u8>,
}
struct Packet {
    header: PacketHeader,
    data: Vec<u8>,
}
struct ClockGuard;
impl Drop for ClockGuard {
    fn drop(&mut self) {
        a2tools_dps_meter_lib::clock::set_override(None);
    }
}
fn read_record(r: &mut impl Read, n: usize) -> Result<Option<Vec<u8>>> {
    let mut first = [0];
    if r.read(&mut first)? == 0 {
        return Ok(None);
    }
    let mut data = vec![0; n];
    data[0] = first[0];
    r.read_exact(&mut data[1..])
        .context("Truncated capture record")?;
    Ok(Some(data))
}
pub fn run(path: &Path) -> Result<Value> {
    let mut reader = BufReader::new(std::fs::File::open(path)?);
    let mut magic = [0; 8];
    reader.read_exact(&mut magic)?;
    let version = match &magic {
        b"A2MCAP1\n" => 1,
        b"A2MCAP2\n" => 2,
        b"A2MCAP3\n" => 3,
        _ => bail!("Unsupported capture format"),
    };
    let legacy = version == 1;
    let mut clock_steps_back = 0u64;
    let mut clock_clamped_packets = 0u64;
    let mut previous_raw_ms = None;
    let engine = Engine::new(Db::in_memory()?, "de", std::env::temp_dir());
    let _guard = ClockGuard;
    let mut flows: HashMap<String, (TcpOrder, StreamAssembler, StreamProcessor, EffectScanner)> =
        HashMap::new();
    let mut packets = 0u64;
    let mut sampled_at = 0;
    let mut last_ms = 0;
    loop {
        let Packet { mut header, data } = if legacy {
            let Some(h) = read_record(&mut reader, 15)? else {
                break;
            };
            let ms = u64::from_le_bytes(h[..8].try_into()?) as i64;
            if h[8] > 1 {
                bail!("Invalid capture direction");
            }
            let port = u16::from_le_bytes(h[9..11].try_into()?);
            let len = u32::from_le_bytes(h[11..15].try_into()?) as usize;
            if len > 2 << 20 {
                bail!("Capture payload too large");
            }
            let mut data = vec![0; len];
            reader.read_exact(&mut data)?;
            Packet {
                header: PacketHeader {
                    ms,
                    from_server: h[8] == 0,
                    src_port: port,
                    dst_port: 0,
                    src_ip: None,
                    dst_ip: None,
                    device: None,
                    seq: 0,
                    ack: 0,
                },
                data,
            }
        } else {
            let Some(h) = read_record(&mut reader, 4)? else {
                break;
            };
            let len = u32::from_le_bytes(h.try_into().unwrap()) as usize;
            let limit = if version == 3 { 64 << 10 } else { 8 << 20 };
            if len == 0 || len > limit {
                bail!("Invalid capture record length");
            }
            let mut data = vec![0; len];
            reader.read_exact(&mut data)?;
            if version == 3 {
                let header = serde_json::from_slice::<PacketHeader>(&data)?;
                let mut len = [0; 4];
                reader
                    .read_exact(&mut len)
                    .context("Truncated capture record")?;
                let len = u32::from_le_bytes(len) as usize;
                if len > 2 << 20 {
                    bail!("Capture payload too large");
                }
                let mut data = vec![0; len];
                reader
                    .read_exact(&mut data)
                    .context("Truncated capture record")?;
                Packet { header, data }
            } else {
                let JsonPacket { header, data } = serde_json::from_slice(&data)?;
                Packet { header, data }
            }
        };
        if header.ms < 0 || data.len() > 2 << 20 {
            bail!("Invalid capture timestamp or payload size");
        }
        // The wall clock can step back (NTP); keep time monotonic instead of
        // rejecting the rest of the recording.
        if previous_raw_ms.is_some_and(|ms| header.ms < ms) {
            clock_steps_back += 1;
        }
        previous_raw_ms = Some(header.ms);
        if header.ms < last_ms {
            clock_clamped_packets += 1;
            header.ms = last_ms;
        }
        last_ms = header.ms;
        packets += 1;
        a2tools_dps_meter_lib::clock::set_override(Some(header.ms));
        let cap = CapturedPayload {
            src_port: header.src_port,
            dst_port: header.dst_port,
            src_ip: header.src_ip.clone(),
            dst_ip: header.dst_ip.clone(),
            device_name: header.device.clone(),
            tcp_seq: header.seq,
            tcp_ack: header.ack,
            captured_at_ms: header.ms,
            data: data.clone(),
        };
        engine.ping.on_packet(
            &cap,
            if header.from_server {
                header.src_port
            } else {
                header.dst_port
            },
        );
        if !header.from_server {
            continue;
        }
        let key = format!(
            "{:?}:{}/ {:?}:{}/{:?}",
            header.src_ip, header.src_port, header.dst_ip, header.dst_port, header.device
        );
        if flows.len() >= 256 && !flows.contains_key(&key) {
            bail!("Too many capture flows");
        }
        let (order, assembler, processor, effects) = flows.entry(key).or_insert_with(|| {
            let mut p = StreamProcessor::new(
                engine.storage.clone(),
                engine.skills.clone(),
                engine.npcs.clone(),
            );
            p.set_dot_skill_ids(engine.dot_skill_ids.clone());
            (
                TcpOrder::default(),
                StreamAssembler::new(),
                p,
                EffectScanner::default(),
            )
        });
        // V1 has no TCP sequence metadata: preserve each port's recorded order.
        // A synthetic global sequence invents holes when ports are interleaved.
        let (chunks, recovered) = if legacy {
            (vec![data], false)
        } else {
            order.feed(header.seq, &data, header.ms)
        };
        if recovered {
            *assembler = StreamAssembler::new();
            *effects = EffectScanner::default();
            engine.capture_gap();
        }
        processor.set_override_timestamp(Some(header.ms));
        for chunk in chunks {
            assembler.process_chunk(&chunk, processor);
            effects.feed(&chunk, |payload| {
                if let Some(event) = crate::buffs::parse(payload) {
                    engine.buffs.record(event, header.ms);
                }
                if let Some(map) = crate::instances::map_load(payload) {
                    engine.note_map_load(map, header.ms);
                }
            });
        }
        if header.ms - sampled_at >= 500 {
            engine.replay_tick();
            sampled_at = header.ms;
        }
    }
    engine.replay_tick();
    let mut report = engine.replay_report();
    report["capture"] = json!({"version":version,"packets":packets,"clock_steps_back":clock_steps_back,"clock_clamped_packets":clock_clamped_packets,"legacy_metadata_limited":legacy,"pending_bytes":flows.values().map(|(o,_,_,_)|o.pending_bytes()).sum::<usize>(),
        "duplicates":flows.values().map(|(o,_,_,_)|o.duplicates).sum::<u64>(),"gaps":flows.values().map(|(o,_,_,_)|o.gaps).sum::<u64>()});
    Ok(report)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatcher::Recorder;

    struct CaptureFile(std::path::PathBuf);
    impl CaptureFile {
        fn new(bytes: &[u8]) -> Self {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let path = std::env::temp_dir().join(format!(
                "a2m-replay-test-{}-{}.a2mcap",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::write(&path, bytes).unwrap();
            Self(path)
        }
        fn run(&self) -> Result<Value> {
            run(&self.0)
        }
    }
    impl Drop for CaptureFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    fn header(ms: i64, seq: u32) -> Value {
        json!({"ms":ms,"from_server":true,"src_port":7777,"dst_port":50000,
            "src_ip":"10.0.0.1","dst_ip":"10.0.0.2","device":"eth0","seq":seq,"ack":0})
    }
    fn json_record(bytes: &mut Vec<u8>, packet: &Value) {
        let json = serde_json::to_vec(packet).unwrap();
        bytes.extend((json.len() as u32).to_le_bytes());
        bytes.extend(json);
    }
    fn legacy_record(bytes: &mut Vec<u8>, ms: u64, port: u16) {
        bytes.extend(ms.to_le_bytes());
        bytes.push(0);
        bytes.extend(port.to_le_bytes());
        bytes.extend(1u32.to_le_bytes());
        bytes.push(0);
    }
    #[test]
    fn v2_requires_payload_but_accepts_an_explicit_empty_payload() {
        let mut bytes = b"A2MCAP2\n".to_vec();
        let mut packet = header(1000, 10);
        json_record(&mut bytes, &packet);
        assert!(CaptureFile::new(&bytes).run().is_err());
        packet["data"] = json!([]);
        bytes.truncate(8);
        json_record(&mut bytes, &packet);
        assert_eq!(
            CaptureFile::new(&bytes).run().unwrap()["capture"]["packets"],
            1
        );
    }
    #[test]
    fn v2_still_decodes_payload_and_tcp_duplicates() {
        let mut bytes = b"A2MCAP2\n".to_vec();
        let mut packet = header(1000, 10);
        packet["data"] = json!([1, 2, 3]);
        json_record(&mut bytes, &packet);
        packet["ms"] = json!(1001);
        json_record(&mut bytes, &packet);
        let report = CaptureFile::new(&bytes).run().unwrap();
        assert_eq!(report["capture"]["version"], 2);
        assert_eq!(report["capture"]["packets"], 2);
        assert_eq!(report["capture"]["duplicates"], 1);
        assert_eq!(report["capture"]["pending_bytes"], 0);
    }
    #[test]
    fn v3_rejects_truncated_headers_lengths_and_payloads() {
        let mut bytes = b"A2MCAP3\n".to_vec();
        json_record(&mut bytes, &header(1000, 10));
        let header_end = bytes.len();
        bytes.extend(3u32.to_le_bytes());
        bytes.extend([1, 2, 3]);
        for end in [
            1,
            7,
            9,
            11,
            12,
            header_end - 1,
            header_end,
            header_end + 1,
            header_end + 3,
            header_end + 4,
            bytes.len() - 1,
        ] {
            assert!(
                CaptureFile::new(&bytes[..end]).run().is_err(),
                "cut at {end}"
            );
        }
        assert_eq!(
            CaptureFile::new(&bytes).run().unwrap()["capture"]["packets"],
            1
        );
        bytes.truncate(header_end);
        bytes.extend(((2u32 << 20) + 1).to_le_bytes());
        let error = CaptureFile::new(&bytes).run().unwrap_err();
        assert!(error.to_string().contains("payload too large"));

        bytes.truncate(8);
        bytes.extend(((64u32 << 10) + 1).to_le_bytes());
        let error = CaptureFile::new(&bytes).run().unwrap_err();
        assert!(error.to_string().contains("record length"));
    }
    #[test]
    fn v3_accepts_empty_raw_payload_and_clears_the_replay_clock_on_error() {
        let mut bytes = b"A2MCAP3\n".to_vec();
        json_record(&mut bytes, &header(1000, 10));
        bytes.extend(0u32.to_le_bytes());
        assert_eq!(
            CaptureFile::new(&bytes).run().unwrap()["capture"]["packets"],
            1
        );
        // Fail after processing a valid packet, while the replay clock is pinned.
        bytes.push(1);
        assert!(CaptureFile::new(&bytes).run().is_err());
        assert!(a2tools_dps_meter_lib::clock::now_ms() > 1_600_000_000_000);
    }
    #[test]
    fn legacy_interleaved_ports_do_not_invent_tcp_gaps() {
        let mut bytes = b"A2MCAP1\n".to_vec();
        for (ms, port) in [(1000, 7777), (1001, 8888), (1002, 7777)] {
            legacy_record(&mut bytes, ms, port);
        }
        let report = CaptureFile::new(&bytes).run().unwrap();
        assert_eq!(report["capture"]["pending_bytes"], 0);
        assert_eq!(report["capture"]["gaps"], 0);
    }
    #[test]
    fn clock_report_counts_steps_separately_from_clamped_packets() {
        let mut bytes = b"A2MCAP3\n".to_vec();
        for ms in [1000, 900, 950, 1000, 1100, 1050, 1100] {
            json_record(&mut bytes, &header(ms, 10));
            bytes.extend(0u32.to_le_bytes());
        }
        let report = CaptureFile::new(&bytes).run().unwrap();
        assert_eq!(report["capture"]["clock_steps_back"], 2);
        assert_eq!(report["capture"]["clock_clamped_packets"], 3);
    }
    #[test]
    fn round_trip_keeps_flow_metadata_and_rejects_truncation() {
        let dir = std::env::temp_dir().join(format!("a2m-replay-{}", std::process::id()));
        let mut rec = Recorder::create(&dir, 1).unwrap();
        let cap = CapturedPayload {
            src_port: 7777,
            dst_port: 50000,
            data: vec![1, 2, 3],
            device_name: Some("eth0".into()),
            src_ip: Some("10.0.0.1".into()),
            dst_ip: Some("10.0.0.2".into()),
            tcp_seq: 10,
            tcp_ack: 0,
            captured_at_ms: 1000,
        };
        rec.write(1000, true, &cap).unwrap();
        rec.write(1001, true, &cap).unwrap();
        let path = rec.path().to_path_buf();
        drop(rec);
        let bytes = std::fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"A2MCAP3\n"));
        assert!(bytes.ends_with(&[3, 0, 0, 0, 1, 2, 3]));
        let report = run(&path).unwrap();
        assert_eq!(report["capture"]["packets"], 2);
        assert_eq!(report["capture"]["duplicates"], 1);
        let mut bytes = std::fs::read(&path).unwrap();
        bytes.pop();
        std::fs::write(&path, bytes).unwrap();
        assert!(run(&path).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
