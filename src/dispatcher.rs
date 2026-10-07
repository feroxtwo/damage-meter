//! Finds the game's connection among everything captured and feeds it to the
//! parser.
//!
//! Adapted from A2Tools DPS Meter's `CaptureDispatcher` (GPL-3.0): before a
//! connection is locked, only server-to-client packets carrying the game's
//! per-record terminator count, and a flow locks once it sustains that marker
//! at the rate only the live game produces.

use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use a2tools_dps_meter_lib::capture::captured_payload::CapturedPayload;
use a2tools_dps_meter_lib::capture::framing::{self, FrameKind};
use a2tools_dps_meter_lib::capture::stream_assembler::StreamAssembler;
use a2tools_dps_meter_lib::capture::stream_processor::StreamProcessor;

use crate::buffs;
use crate::engine::Engine;
use crate::tcp::TcpOrder;

const COMBAT_SIGNATURES: [&[u8]; 2] = [&[0x0E, 0x00, 0x36], &[0x06, 0x00, 0x36]];
const SIGNATURE_LOCK_THRESHOLD: u32 = 12;
const SIGNATURE_WINDOW_MS: i64 = 3_000;
const STALE_CONNECTION_MS: i64 = 120_000;
const PROCESS_CHECK_STOPPED_MS: i64 = 10_000;
const PROCESS_CHECK_RUNNING_MS: i64 = 60_000;
/// More unframed bytes than this means the walk lost the stream.
const MAX_SCAN_BUFFER: usize = 2 << 20;
/// Bundles inside bundles, at most this deep.
const MAX_BUNDLE_DEPTH: u8 = 3;
/// Streams watched for the game at once; past this the quietest is dropped.
const MAX_CANDIDATE_FLOWS: usize = 32;
/// A watched stream silent this long is forgotten.
const CANDIDATE_IDLE_MS: i64 = 60_000;
/// Recent payloads kept per watched stream, replayed when it locks.
const CANDIDATE_BUFFER_BYTES: usize = 1 << 20;
/// A locked connection silent this long may hand over to another stream.
const LOCK_QUIET_MS: i64 = 3_000;

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
        if !entry
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|b| b.is_ascii_digit())
        {
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

/// Walks the server's stream for the packets the A2Tools parser skips (buff
/// and debuff effects), with the same framing it uses.
#[derive(Default)]
pub(crate) struct EffectScanner {
    buffer: Vec<u8>,
}

fn visit_inner(data: &[u8], depth: u8, f: &mut impl FnMut(&[u8])) {
    for frame in framing::walk_inner(data).frames {
        match frame.kind {
            FrameKind::Packet => f(frame.payload(data)),
            FrameKind::Bundle if depth < MAX_BUNDLE_DEPTH => {
                if let Some(inner) = framing::decompress_bundle(frame.payload(data)) {
                    visit_inner(&inner, depth + 1, f);
                }
            }
            FrameKind::Bundle => {}
        }
    }
}

impl EffectScanner {
    /// Call `f` with each complete packet's payload (opcode first).
    pub(crate) fn feed(&mut self, data: &[u8], mut f: impl FnMut(&[u8])) {
        self.buffer.extend_from_slice(data);
        let walked = framing::walk(&self.buffer);
        for frame in &walked.frames {
            match frame.kind {
                FrameKind::Packet => f(frame.payload(&self.buffer)),
                FrameKind::Bundle => {
                    if let Some(inner) = framing::decompress_bundle(frame.payload(&self.buffer)) {
                        visit_inner(&inner, 1, &mut f);
                    }
                }
            }
        }
        self.buffer.drain(..walked.consumed.min(self.buffer.len()));
        if self.buffer.len() > MAX_SCAN_BUFFER {
            self.buffer.clear();
        }
    }
}

/// Recordings kept at most, all files together; the oldest go first.
const MAX_CAPTURE_BYTES: u64 = 2 << 30;

/// Delete the oldest recordings until the rest fit in `keep` bytes.
fn prune_captures(dir: &std::path::Path, keep: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "a2mcap"))
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            Some((m.modified().ok()?, m.len(), e.path()))
        })
        .collect();
    files.sort();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    for (_, len, path) in files {
        if total <= keep {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            tracing::info!("Deleted old recording {}", path.display());
            total -= len;
        }
    }
}

/// Packet recordings (`aion2-meter ctl record`): the game connection's raw
/// TCP payloads, for working out packets no meter decodes yet.
///
/// File: `A2MCAP3\n`, then per packet `u32 LE header length`, a JSON header
/// (timestamp, direction, endpoints, interface, TCP sequence and
/// acknowledgement), `u32 LE payload length` and the raw payload. Version 2
/// wrote the payload into the JSON as a number array, three to four times its
/// size; the reader still accepts v1 and v2.
pub struct Recorder {
    out: BufWriter<File>,
    path: PathBuf,
    last_flush: i64,
}

impl Recorder {
    pub const MAGIC: &'static [u8] = b"A2MCAP3\n";

    pub fn create(dir: &std::path::Path, now: i64) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        prune_captures(dir, MAX_CAPTURE_BYTES);
        let path = dir.join(format!("aion2-{now}.a2mcap"));
        let mut out = BufWriter::new(File::options().write(true).create_new(true).open(&path)?);
        out.write_all(Self::MAGIC)?;
        Ok(Self {
            out,
            path,
            last_flush: now,
        })
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub fn write(
        &mut self,
        now: i64,
        from_server: bool,
        cap: &CapturedPayload,
    ) -> std::io::Result<()> {
        let header = serde_json::to_vec(&serde_json::json!({
            "ms": now,
            "from_server": from_server,
            "src_port": cap.src_port,
            "dst_port": cap.dst_port,
            "src_ip": cap.src_ip,
            "dst_ip": cap.dst_ip,
            "device": cap.device_name,
            "seq": cap.tcp_seq,
            "ack": cap.tcp_ack,
        }))?;
        self.out.write_all(&(header.len() as u32).to_le_bytes())?;
        self.out.write_all(&header)?;
        self.out.write_all(&(cap.data.len() as u32).to_le_bytes())?;
        self.out.write_all(&cap.data)?;
        if now - self.last_flush >= 1000 {
            self.out.flush()?;
            self.last_flush = now;
        }
        Ok(())
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        let _ = self.out.flush();
    }
}

/// Directional server-to-client identity. Ports alone are not unique across
/// hosts or interfaces and must never mix separate TCP streams.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct FlowKey {
    server_ip: Option<String>,
    client_ip: Option<String>,
    server_port: u16,
    client_port: u16,
    device: String,
}

impl FlowKey {
    fn from_server(cap: &CapturedPayload) -> Self {
        Self {
            server_ip: cap.src_ip.clone(),
            client_ip: cap.dst_ip.clone(),
            server_port: cap.src_port,
            client_port: cap.dst_port,
            device: cap.device_name.clone().unwrap_or_default(),
        }
    }

    fn direction(&self, cap: &CapturedPayload) -> Option<bool> {
        if cap.device_name.as_deref().unwrap_or_default() != self.device {
            return None;
        }
        if cap.src_ip == self.server_ip
            && cap.dst_ip == self.client_ip
            && cap.src_port == self.server_port
            && cap.dst_port == self.client_port
        {
            Some(true)
        } else if cap.dst_ip == self.server_ip
            && cap.src_ip == self.client_ip
            && cap.dst_port == self.server_port
            && cap.src_port == self.client_port
        {
            Some(false)
        } else {
            None
        }
    }
}

/// Every stream that is not the locked game connection, with what it sent
/// recently. Nothing is parsed before a stream is known to be the game's, but
/// its recent payloads are replayed when it is: the monsters of an instance
/// spawn when you enter, long before the first fight shows the combat marker,
/// and without those packets the meter cannot name them.
///
/// While locked, the same rule lets a new connection (reconnect, server
/// change) take over once the old one has gone quiet.
#[derive(Default)]
struct Candidates {
    flows: HashMap<FlowKey, Candidate>,
}

#[derive(Default)]
struct Candidate {
    hits: u32,
    last_hit: i64,
    last_seen: i64,
    /// Looked like TLS: never the game, nothing buffered.
    tls: bool,
    chunks: VecDeque<CapturedPayload>,
    order: TcpOrder,
    bytes: usize,
}

impl Candidates {
    /// Returns the stream and its buffered payloads (oldest first) once it
    /// should become the game connection. `may_lock` is false while a locked
    /// connection is still talking.
    fn observe(
        &mut self,
        cap: CapturedPayload,
        now: i64,
        may_lock: bool,
    ) -> Option<(FlowKey, Vec<CapturedPayload>)> {
        let key = FlowKey::from_server(&cap);
        if !self.flows.contains_key(&key) {
            self.flows
                .retain(|_, c| now - c.last_seen <= CANDIDATE_IDLE_MS);
            if self.flows.len() >= MAX_CANDIDATE_FLOWS {
                // Make room by forgetting the stream silent the longest.
                let oldest = self
                    .flows
                    .iter()
                    .min_by_key(|(_, c)| c.last_seen)
                    .map(|(k, _)| k.clone())?;
                self.flows.remove(&oldest);
            }
        }
        let c = self.flows.entry(key.clone()).or_default();
        c.last_seen = now;
        if c.tls || looks_like_tls(&cap.data) {
            c.tls = true;
            c.chunks.clear();
            c.bytes = 0;
            return None;
        }
        let (ordered, _) = c.order.feed(cap.tcp_seq, &cap.data, now);
        if ordered.iter().any(|chunk| contains_signature(chunk)) {
            if now - c.last_hit > SIGNATURE_WINDOW_MS {
                c.hits = 0;
            }
            c.hits += 1;
            c.last_hit = now;
        }
        c.bytes += cap.data.len();
        c.chunks.push_back(cap);
        while c.bytes > CANDIDATE_BUFFER_BYTES {
            match c.chunks.pop_front() {
                Some(old) => c.bytes -= old.data.len(),
                None => break,
            }
        }
        if c.hits >= SIGNATURE_LOCK_THRESHOLD && may_lock {
            let c = self.flows.remove(&key)?;
            self.flows.clear();
            return Some((key, c.chunks.into()));
        }
        None
    }
}

pub struct Dispatcher {
    engine: Arc<Engine>,
    /// Skip the "is AION2.exe running" check (for a game in a container or
    /// VM, or under a launcher that renames it).
    require_process: bool,
}

impl Dispatcher {
    pub fn new(engine: Arc<Engine>, require_process: bool) -> Self {
        Self {
            engine,
            require_process,
        }
    }

    pub fn run(self, rx: Receiver<CapturedPayload>) {
        let mut flows: HashMap<
            FlowKey,
            (StreamAssembler, StreamProcessor, EffectScanner, TcpOrder),
        > = HashMap::new();
        let mut recorder: Option<Recorder> = None;
        let mut candidates = Candidates::default();
        let mut lock: Option<FlowKey> = None;
        let mut last_lock_packet_ms = 0i64;
        let mut last_parsed_ms = 0i64;
        let mut last_process_check = 0i64;
        let mut game = !self.require_process;

        loop {
            // Check process, recording and connection state even on a quiet
            // network, instead of waiting forever for another payload.
            let cap = match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(cap) => Some(cap),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => break,
            };
            let now = now_ms();

            if self.require_process {
                let interval = if game {
                    PROCESS_CHECK_RUNNING_MS
                } else {
                    PROCESS_CHECK_STOPPED_MS
                };
                if now - last_process_check >= interval {
                    last_process_check = now;
                    let running = game_running();
                    if running != game {
                        tracing::info!(
                            "AION2.exe {}",
                            if running {
                                "found"
                            } else {
                                "no longer running"
                            }
                        );
                    }
                    if !running && game {
                        lock = None;
                        flows.clear();
                        candidates = Candidates::default();
                        self.engine.ping.reset();
                    }
                    game = running;
                    self.engine.set_game_running(running);
                }
            }
            if !game {
                recorder = None;
                self.engine.recording_paused();
                continue;
            }

            if lock.is_some() && last_parsed_ms > 0 && now - last_parsed_ms > STALE_CONNECTION_MS {
                tracing::info!(
                    "Nothing parsed for {} s, unlocking",
                    (now - last_parsed_ms) / 1000
                );
                lock = None;
                flows.clear();
                self.engine.ping.reset();
                self.engine.set_locked(None);
            }

            if lock.is_some() {
                self.sync_recorder(&mut recorder, now);
            } else {
                recorder = None;
                self.engine.recording_paused();
            }

            let Some(cap) = cap else {
                continue;
            };
            let Some(from_server) = lock.as_ref().and_then(|l| l.direction(&cap)) else {
                let may_lock = lock.is_none() || now - last_lock_packet_ms >= LOCK_QUIET_MS;
                if let Some((key, chunks)) = candidates.observe(cap, now, may_lock) {
                    if lock.is_some() {
                        tracing::info!(
                            "Game connection moved to server port {} on {}",
                            key.server_port,
                            key.device
                        );
                        self.engine.ping.reset();
                    } else {
                        tracing::info!(
                            "Locked onto game server port {} on {}",
                            key.server_port,
                            key.device
                        );
                    }
                    flows.clear();
                    self.engine
                        .set_locked(Some((key.server_port, key.device.clone())));
                    let (assembler, processor, effects, order) =
                        flows.entry(key.clone()).or_insert_with(|| self.new_flow());
                    // Keep the pre-lock spawn packets in recordings as well.
                    recorder = None;
                    self.sync_recorder(&mut recorder, now);
                    for packet in &chunks {
                        if let Some(rec) = &mut recorder
                            && let Err(e) = rec.write(packet.captured_at_ms, true, packet)
                        {
                            recorder = None;
                            self.engine.stop_recording(Some(e.to_string()));
                        }
                        let (ordered, recovered) =
                            order.feed(packet.tcp_seq, &packet.data, packet.captured_at_ms);
                        if recovered {
                            *assembler = StreamAssembler::new();
                            *effects = EffectScanner::default();
                            self.engine.capture_gap();
                        }
                        processor.set_override_timestamp(Some(packet.captured_at_ms));
                        for chunk in ordered {
                            assembler.process_chunk(&chunk, processor);
                            effects.feed(&chunk, |payload| {
                                if let Some(event) = buffs::parse(payload) {
                                    self.engine.buffs.record(event, packet.captured_at_ms);
                                }
                            });
                        }
                    }
                    processor.set_override_timestamp(None);
                    lock = Some(key);
                    last_parsed_ms = now;
                    last_lock_packet_ms = now;
                }
                continue;
            };
            let Some(l) = lock.as_ref() else {
                continue;
            };
            last_lock_packet_ms = now;
            if let Some(rec) = &mut recorder
                && let Err(e) = rec.write(cap.captured_at_ms, from_server, &cap)
            {
                tracing::error!("Recording failed: {e}");
                recorder = None;
                self.engine.stop_recording(Some(e.to_string()));
            }
            let before = self.engine.ping.current_ping_ms();
            self.engine.ping.on_packet(&cap, l.server_port);
            if self.engine.ping.current_ping_ms() != before {
                last_parsed_ms = now;
            }
            if !from_server {
                continue; // only server -> client carries combat
            }

            let (assembler, processor, effects, order) =
                flows.entry(l.clone()).or_insert_with(|| self.new_flow());
            let (chunks, recovered) = order.feed(cap.tcp_seq, &cap.data, now);
            if recovered {
                *assembler = StreamAssembler::new();
                *effects = EffectScanner::default();
                self.engine.capture_gap();
            }
            for chunk in chunks {
                if assembler.process_chunk(&chunk, processor) {
                    last_parsed_ms = now;
                }
                effects.feed(&chunk, |payload| {
                    if let Some(event) = buffs::parse(payload) {
                        self.engine.buffs.record(event, now);
                    }
                });
            }
        }
    }
}

impl Dispatcher {
    fn new_flow(&self) -> (StreamAssembler, StreamProcessor, EffectScanner, TcpOrder) {
        let mut p = StreamProcessor::new(
            self.engine.storage.clone(),
            self.engine.skills.clone(),
            self.engine.npcs.clone(),
        );
        p.set_dot_skill_ids(self.engine.dot_skill_ids.clone());
        (
            StreamAssembler::new(),
            p,
            EffectScanner::default(),
            TcpOrder::default(),
        )
    }

    /// Open or close the recording file when `ctl record` flipped it.
    fn sync_recorder(&self, recorder: &mut Option<Recorder>, now: i64) {
        let wanted = self.engine.recording_wanted();
        if wanted && recorder.is_none() {
            match Recorder::create(&self.engine.capture_dir(), now) {
                Ok(r) => {
                    tracing::info!("Recording packets to {}", r.path().display());
                    self.engine.recording_started(r.path());
                    *recorder = Some(r);
                }
                Err(e) => {
                    tracing::error!("Cannot record: {e}");
                    self.engine.stop_recording(Some(e.to_string()));
                }
            }
        } else if !wanted
            && recorder.is_some()
            && let Some(r) = recorder.take()
        {
            tracing::info!("Recording saved: {}", r.path().display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(payload: &[u8]) -> Vec<u8> {
        let len = framing::length_value(payload.len());
        assert!(len < 0x80);
        let mut f = vec![len as u8];
        f.extend(payload);
        f
    }

    #[test]
    fn scanner_finds_packets_split_across_reads() {
        let a = frame(&[0x2A, 0x38, 1, 2, 3]);
        let b = frame(&[0x04, 0x38, 9, 9]);
        let stream: Vec<u8> = a.iter().chain(&b).copied().collect();
        let mut scanner = EffectScanner::default();
        let mut seen: Vec<Vec<u8>> = Vec::new();
        scanner.feed(&stream[..4], |p| seen.push(p.to_vec()));
        assert!(seen.is_empty());
        scanner.feed(&stream[4..], |p| seen.push(p.to_vec()));
        assert_eq!(
            seen,
            vec![vec![0x2A, 0x38, 1, 2, 3], vec![0x04, 0x38, 9, 9]]
        );
        assert!(scanner.buffer.is_empty());
    }

    #[test]
    fn old_recordings_make_room() {
        let dir = std::env::temp_dir().join(format!("a2m-prune-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for (i, name) in ["a.a2mcap", "b.a2mcap", "c.a2mcap"].iter().enumerate() {
            std::fs::write(dir.join(name), vec![0u8; 100]).unwrap();
            let t =
                std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(i as u64 + 1);
            File::options()
                .write(true)
                .open(dir.join(name))
                .unwrap()
                .set_modified(t)
                .unwrap();
        }
        std::fs::write(dir.join("notes.txt"), vec![0u8; 500]).unwrap();
        prune_captures(&dir, 250);
        let mut left: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(left, ["b.a2mcap", "c.a2mcap", "notes.txt"]);
    }

    #[test]
    fn recorder_never_overwrites_a_capture() {
        let dir = std::env::temp_dir().join(format!("a2m-v2-rec-{}", std::process::id()));
        let r = Recorder::create(&dir, 42).unwrap();
        assert!(Recorder::create(&dir, 42).is_err());
        let path = r.path().to_path_buf();
        drop(r);
        assert_eq!(std::fs::read(path).unwrap(), Recorder::MAGIC);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn finds_the_game_under_proton() {
        assert!(is_aion2_process("AION2.exe", b""));
        assert!(is_aion2_process(
            "wine64-preload",
            b"Z:\\home\\p\\.steam\\steamapps\\common\\AION2\\Binaries\\Win64\\AION2.exe\0-arg\0"
        ));
        assert!(!is_aion2_process(
            "python3",
            b"python3\0/proton\0run\0Z:\\games\\AION2.exe\0"
        ));
    }

    fn server_payload(client_port: u16, data: &[u8]) -> CapturedPayload {
        CapturedPayload {
            src_ip: Some("10.0.0.1".into()),
            dst_ip: Some("192.168.1.2".into()),
            src_port: 7777,
            dst_port: client_port,
            device_name: Some("eth0".into()),
            tcp_seq: 1,
            tcp_ack: 0,
            captured_at_ms: 0,
            data: data.to_vec(),
        }
    }

    #[test]
    fn packets_before_the_lock_are_replayed() {
        let mut c = Candidates::default();
        let combat = [0x0E, 0x00, 0x36];
        // Spawn packets arrive long before the first fight.
        assert!(c.observe(server_payload(50001, &[7]), 0, true).is_none());
        // A TLS stream is never buffered.
        let tls = server_payload(50002, &[0x17, 0x03, 0x03, 0, 1]);
        assert!(c.observe(tls, 0, true).is_none());
        assert!(c.observe(server_payload(50002, &combat), 1, true).is_none());
        assert!(
            c.flows[&FlowKey::from_server(&server_payload(50002, &[]))]
                .chunks
                .is_empty()
        );
        for i in 0..SIGNATURE_LOCK_THRESHOLD - 1 {
            let mut cap = server_payload(50001, &combat);
            cap.tcp_seq = 2 + 3 * i;
            assert!(c.observe(cap, 30_000 + i as i64, true).is_none());
        }
        let (key, chunks) = c
            .observe(
                {
                    let mut p = server_payload(50001, &combat);
                    p.tcp_seq = 2 + 3 * (SIGNATURE_LOCK_THRESHOLD - 1);
                    p
                },
                30_100,
                true,
            )
            .expect("locks");
        assert_eq!(key.client_port, 50001);
        assert_eq!(chunks.len(), SIGNATURE_LOCK_THRESHOLD as usize + 1);
        assert_eq!(chunks[0].data, vec![7]);
        assert!(c.flows.is_empty());
    }

    #[test]
    fn a_reconnect_waits_until_the_old_connection_is_quiet() {
        let mut c = Candidates::default();
        let combat = [0x0E, 0x00, 0x36];
        for i in 0..SIGNATURE_LOCK_THRESHOLD {
            assert!(
                c.observe(
                    {
                        let mut p = server_payload(50001, &combat);
                        p.tcp_seq = 1 + 3 * i;
                        p
                    },
                    i as i64,
                    false
                )
                .is_none()
            );
        }
        assert!(
            c.observe(
                {
                    let mut p = server_payload(50001, &combat);
                    p.tcp_seq = 1 + 3 * SIGNATURE_LOCK_THRESHOLD;
                    p
                },
                20,
                true
            )
            .is_some()
        );
    }

    #[test]
    fn candidate_buffers_and_count_are_bounded() {
        let mut c = Candidates::default();
        for port in 0..MAX_CANDIDATE_FLOWS as u16 + 5 {
            c.observe(server_payload(40000 + port, &[1]), port as i64, true);
        }
        assert_eq!(c.flows.len(), MAX_CANDIDATE_FLOWS);
        let big = vec![1u8; 300_000];
        for _ in 0..10 {
            c.observe(server_payload(60000, &big), 100, true);
        }
        let flow = &c.flows[&FlowKey::from_server(&server_payload(60000, &[]))];
        assert!(flow.bytes <= CANDIDATE_BUFFER_BYTES && flow.chunks.len() == 3);
    }

    #[test]
    fn connection_identity_includes_hosts_ports_and_interface() {
        let cap = CapturedPayload {
            src_ip: Some("10.0.0.1".into()),
            dst_ip: Some("192.168.1.2".into()),
            src_port: 7777,
            dst_port: 50000,
            device_name: Some("eth0".into()),
            tcp_seq: 1,
            tcp_ack: 0,
            captured_at_ms: 0,
            data: vec![1],
        };
        let key = FlowKey::from_server(&cap);
        assert_eq!(key.direction(&cap), Some(true));
        let mut other = cap.clone();
        std::mem::swap(&mut other.src_ip, &mut other.dst_ip);
        std::mem::swap(&mut other.src_port, &mut other.dst_port);
        assert_eq!(key.direction(&other), Some(false));
        other = cap.clone();
        other.src_ip = Some("10.0.0.2".into());
        assert_eq!(key.direction(&other), None);
        assert_ne!(key, FlowKey::from_server(&other));
        other = cap.clone();
        other.dst_port += 1;
        assert_eq!(key.direction(&other), None);
        other = cap.clone();
        other.device_name = Some("tun0".into());
        assert_eq!(key.direction(&other), None);
    }

    #[test]
    fn tls_and_signatures() {
        assert!(looks_like_tls(&[0x17, 0x03, 0x03, 0x00]));
        assert!(!looks_like_tls(&[0x17, 0x36]));
        assert!(contains_signature(&[1, 2, 0x0E, 0x00, 0x36, 4]));
        assert!(!contains_signature(&[0x0E, 0x00, 0x37]));
    }
    #[test]
    fn duplicate_markers_cannot_lock_a_candidate() {
        let mut c = Candidates::default();
        for i in 0..20 {
            assert!(
                c.observe(server_payload(50001, &[0x0e, 0, 0x36]), i, true)
                    .is_none()
            );
        }
        assert_eq!(c.flows.values().next().unwrap().hits, 1);
    }
}
