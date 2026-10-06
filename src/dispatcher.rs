//! Finds the game's connection among everything captured and feeds it to the
//! parser.
//!
//! Adapted from A2Tools DPS Meter's `CaptureDispatcher` (GPL-3.0): before a
//! connection is locked, only server-to-client packets carrying the game's
//! per-record terminator count, and a flow locks once it sustains that marker
//! at the rate only the live game produces.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::{SystemTime, UNIX_EPOCH};

use a2tools_dps_meter_lib::capture::captured_payload::CapturedPayload;
use a2tools_dps_meter_lib::capture::stream_assembler::StreamAssembler;
use a2tools_dps_meter_lib::capture::stream_processor::StreamProcessor;

use crate::engine::Engine;

const COMBAT_SIGNATURES: [&[u8]; 2] = [&[0x0E, 0x00, 0x36], &[0x06, 0x00, 0x36]];
const SIGNATURE_LOCK_THRESHOLD: u32 = 12;
const SIGNATURE_WINDOW_MS: i64 = 3_000;
const STALE_CONNECTION_MS: i64 = 120_000;
const PROCESS_CHECK_STOPPED_MS: i64 = 10_000;
const PROCESS_CHECK_RUNNING_MS: i64 = 60_000;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Whether a process is the game. Under Proton it is a Wine process named
/// `AION2.exe`, or one whose argv[0] is a path ending in it.
pub fn is_aion2_process(comm: &str, cmdline: &[u8]) -> bool {
    let is_game = |name: &str| name.trim().eq_ignore_ascii_case("AION2.exe");
    if is_game(comm) {
        return true;
    }
    let argv0 = cmdline.split(|&b| b == 0).next().unwrap_or(&[]);
    let argv0 = String::from_utf8_lossy(argv0);
    argv0.rsplit(['/', '\\']).next().is_some_and(is_game)
}

pub fn game_running() -> bool {
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return false;
    };
    dir.flatten().any(|entry| {
        let path = entry.path();
        if !entry.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        let comm = std::fs::read_to_string(path.join("comm")).unwrap_or_default();
        let cmdline = std::fs::read(path.join("cmdline")).unwrap_or_default();
        is_aion2_process(&comm, &cmdline)
    })
}

fn looks_like_tls(data: &[u8]) -> bool {
    data.len() >= 3 && (0x14..=0x17).contains(&data[0]) && data[1] == 0x03 && data[2] <= 0x04
}

fn contains_signature(data: &[u8]) -> bool {
    COMBAT_SIGNATURES
        .iter()
        .any(|sig| data.windows(sig.len()).any(|w| w == *sig))
}

#[derive(Default)]
struct Lock {
    port: u16,
    device: String,
}

pub struct Dispatcher {
    engine: Arc<Engine>,
    /// Skip the "is AION2.exe running" check (for a game in a container or
    /// VM, or under a launcher that renames it).
    require_process: bool,
}

impl Dispatcher {
    pub fn new(engine: Arc<Engine>, require_process: bool) -> Self {
        Self { engine, require_process }
    }

    pub fn run(self, rx: Receiver<CapturedPayload>) {
        let mut flows: HashMap<(u16, u16), (StreamAssembler, StreamProcessor)> = HashMap::new();
        let mut sig_hits: HashMap<(u16, u16), (u32, i64)> = HashMap::new();
        let mut lock: Option<Lock> = None;
        let mut last_parsed_ms = 0i64;
        let mut last_process_check = 0i64;
        let mut game = !self.require_process;

        while let Ok(cap) = rx.recv() {
            let now = now_ms();

            if self.require_process {
                let interval = if game { PROCESS_CHECK_RUNNING_MS } else { PROCESS_CHECK_STOPPED_MS };
                if now - last_process_check >= interval {
                    last_process_check = now;
                    let running = game_running();
                    if running != game {
                        tracing::info!("AION2.exe {}", if running { "found" } else { "no longer running" });
                    }
                    if !running && game {
                        lock = None;
                        flows.clear();
                        sig_hits.clear();
                        self.engine.ping.reset();
                    }
                    game = running;
                    self.engine.set_game_running(running);
                }
            }
            if !game {
                continue;
            }

            if lock.is_some() && last_parsed_ms > 0 && now - last_parsed_ms > STALE_CONNECTION_MS {
                tracing::info!("Nothing parsed for {} s, unlocking", (now - last_parsed_ms) / 1000);
                lock = None;
                flows.clear();
                sig_hits.clear();
                self.engine.ping.reset();
                self.engine.set_locked(None);
            }

            let device = cap.device_name.clone().unwrap_or_default();
            if let Some(l) = &lock {
                if l.device != device || (cap.src_port != l.port && cap.dst_port != l.port) {
                    continue;
                }
                let before = self.engine.ping.current_ping_ms();
                self.engine.ping.on_packet(&cap, l.port);
                if self.engine.ping.current_ping_ms() != before {
                    last_parsed_ms = now;
                }
                if cap.src_port != l.port {
                    continue; // only server -> client carries combat
                }
            } else if looks_like_tls(&cap.data) || !contains_signature(&cap.data) {
                continue;
            }

            let key = (cap.src_port.min(cap.dst_port), cap.src_port.max(cap.dst_port));
            let (assembler, processor) = flows.entry(key).or_insert_with(|| {
                let mut p = StreamProcessor::new(
                    self.engine.storage.clone(),
                    self.engine.skills.clone(),
                    self.engine.npcs.clone(),
                );
                p.set_dot_skill_ids(self.engine.dot_skill_ids.clone());
                (StreamAssembler::new(), p)
            });

            if lock.is_none() {
                let slot = sig_hits.entry(key).or_insert((0, now));
                if now - slot.1 > SIGNATURE_WINDOW_MS {
                    slot.0 = 0;
                }
                slot.0 += 1;
                slot.1 = now;
            }

            if assembler.process_chunk(&cap.data, processor) {
                last_parsed_ms = now;
            }

            if lock.is_none() && sig_hits.get(&key).is_some_and(|(c, _)| *c >= SIGNATURE_LOCK_THRESHOLD) {
                tracing::info!("Locked onto game server port {} on {}", cap.src_port, device);
                lock = Some(Lock { port: cap.src_port, device: device.clone() });
                last_parsed_ms = now;
                flows.retain(|k, _| *k == key);
                sig_hits.clear();
                self.engine.set_locked(Some((cap.src_port, device)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_game_under_proton() {
        assert!(is_aion2_process("AION2.exe", b""));
        assert!(is_aion2_process(
            "wine64-preload",
            b"Z:\\home\\p\\.steam\\steamapps\\common\\AION2\\Binaries\\Win64\\AION2.exe\0-arg\0"
        ));
        assert!(!is_aion2_process("python3", b"python3\0/proton\0run\0Z:\\games\\AION2.exe\0"));
    }

    #[test]
    fn tls_and_signatures() {
        assert!(looks_like_tls(&[0x17, 0x03, 0x03, 0x00]));
        assert!(!looks_like_tls(&[0x17, 0x36]));
        assert!(contains_signature(&[1, 2, 0x0E, 0x00, 0x36, 4]));
        assert!(!contains_signature(&[0x0E, 0x00, 0x37]));
    }
}
