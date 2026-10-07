//! Offline, deterministic decoder for v1 and v2 meter captures.
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
struct Packet {
    ms: i64,
    from_server: bool,
    src_port: u16,
    dst_port: u16,
    src_ip: Option<String>,
    dst_ip: Option<String>,
    device: Option<String>,
    seq: u32,
    ack: u32,
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
    let legacy = &magic == b"A2MCAP1\n";
    if !legacy && &magic != b"A2MCAP2\n" {
        bail!("Unsupported capture format");
    }
    let engine = Engine::new(Db::in_memory()?, "de", std::env::temp_dir());
    let _guard = ClockGuard;
    let mut flows: HashMap<String, (TcpOrder, StreamAssembler, StreamProcessor, EffectScanner)> =
        HashMap::new();
    let mut packets = 0u64;
    let mut legacy_seq = 0u32;
    let mut sampled_at = 0;
    let mut last_ms = 0;
    loop {
        let packet = if legacy {
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
            let seq = legacy_seq;
            if h[8] == 0 {
                legacy_seq = legacy_seq.wrapping_add(len as u32);
            }
            Packet {
                ms,
                from_server: h[8] == 0,
                src_port: port,
                dst_port: 0,
                src_ip: None,
                dst_ip: None,
                device: None,
                seq,
                ack: 0,
                data,
            }
        } else {
            let Some(h) = read_record(&mut reader, 4)? else {
                break;
            };
            let len = u32::from_le_bytes(h.try_into().unwrap()) as usize;
            if len == 0 || len > 8 << 20 {
                bail!("Invalid capture record length");
            }
            let mut data = vec![0; len];
            reader.read_exact(&mut data)?;
            serde_json::from_slice::<Packet>(&data)?
        };
        if packet.ms < 0 || packet.ms < last_ms || packet.data.len() > 2 << 20 {
            bail!("Invalid capture timestamp or payload size");
        }
        last_ms = packet.ms;
        packets += 1;
        a2tools_dps_meter_lib::clock::set_override(Some(packet.ms));
        let cap = CapturedPayload {
            src_port: packet.src_port,
            dst_port: packet.dst_port,
            src_ip: packet.src_ip.clone(),
            dst_ip: packet.dst_ip.clone(),
            device_name: packet.device.clone(),
            tcp_seq: packet.seq,
            tcp_ack: packet.ack,
            captured_at_ms: packet.ms,
            data: packet.data.clone(),
        };
        engine.ping.on_packet(
            &cap,
            if packet.from_server {
                packet.src_port
            } else {
                packet.dst_port
            },
        );
        if !packet.from_server {
            continue;
        }
        let key = format!(
            "{:?}:{}/ {:?}:{}/{:?}",
            packet.src_ip, packet.src_port, packet.dst_ip, packet.dst_port, packet.device
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
        let (chunks, recovered) = order.feed(packet.seq, &packet.data, packet.ms);
        if recovered {
            *assembler = StreamAssembler::new();
            *effects = EffectScanner::default();
            engine.capture_gap();
        }
        processor.set_override_timestamp(Some(packet.ms));
        for chunk in chunks {
            assembler.process_chunk(&chunk, processor);
            effects.feed(&chunk, |payload| {
                if let Some(event) = crate::buffs::parse(payload) {
                    engine.buffs.record(event, packet.ms);
                }
            });
        }
        if packet.ms - sampled_at >= 500 {
            engine.replay_tick();
            sampled_at = packet.ms;
        }
    }
    engine.replay_tick();
    let mut report = engine.replay_report();
    report["capture"] = json!({"version":if legacy{1}else{2},"packets":packets,"legacy_metadata_limited":legacy,"pending_bytes":flows.values().map(|(o,_,_,_)|o.pending_bytes()).sum::<usize>(),
        "duplicates":flows.values().map(|(o,_,_,_)|o.duplicates).sum::<u64>(),"gaps":flows.values().map(|(o,_,_,_)|o.gaps).sum::<u64>()});
    Ok(report)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatcher::Recorder;
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
