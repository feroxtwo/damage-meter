//! Distributable offline reference database. Only aggregate snapshots, no fights.
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OpenFlags, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::community::CommunitySnapshot;

const APPLICATION_ID: i64 = 0x41325246;
const MAX_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Debug, Deserialize, Serialize)]
pub struct ProviderArchive {
    pub schema: String,
    pub source: crate::community::ReferenceSource,
    pub evidence: Vec<Value>,
    pub detail: String,
    pub rows: Vec<Observation>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Observation {
    pub class_key: String,
    pub region: Option<String>,
    pub dungeon_id: Option<i32>,
    pub mob_code: Option<i32>,
    pub cp_min: Option<i64>,
    pub cp_max: Option<i64>,
    pub median_dps: Option<f64>,
    pub samples: Option<u32>,
    pub metric: String,
    pub scope: Value,
    pub statistics: Value,
    pub compatibility: Vec<String>,
}

impl ProviderArchive {
    fn validate(self) -> Result<Self> {
        self.source
            .validate_provenance()
            .map_err(anyhow::Error::msg)?;
        if self.schema != "a2m-provider-observations-v1"
            || self.rows.is_empty()
            || self.rows.len() > 20_000
            || self.evidence.is_empty()
            || self.evidence.len() > 100
            || self.detail.len() > 2000
        {
            bail!("Invalid provider observation archive");
        }
        for row in &self.rows {
            if !crate::community::valid_class(&row.class_key)
                || row
                    .region
                    .as_deref()
                    .is_some_and(|r| !crate::community::valid_region(r))
                || row.dungeon_id.is_some_and(|v| v < 0)
                || row.mob_code.is_some_and(|v| v <= 0)
                || row.cp_min.is_some_and(|v| v <= 0)
                || row.cp_max.is_some_and(|v| v <= 0)
                || row.cp_min.zip(row.cp_max).is_some_and(|(a, b)| a > b)
                || row
                    .median_dps
                    .is_some_and(|v| !v.is_finite() || v < 0.0 || v > 1e9)
                || row.metric.is_empty()
                || row.metric.len() > 100
                || !row.scope.is_object()
                || !row.statistics.is_object()
                || row.compatibility.is_empty()
            {
                bail!("Invalid normalized observation");
            }
        }
        Ok(self)
    }
}

pub struct Bundle {
    pub snapshots: Vec<CommunitySnapshot>,
    pub archives: Vec<ProviderArchive>,
}

fn decode(payload: &str) -> Result<CommunitySnapshot> {
    serde_json::from_str::<CommunitySnapshot>(payload)?
        .validate()
        .map_err(|reason| anyhow::anyhow!(reason))
}

pub fn build(output: &Path, inputs: &[PathBuf]) -> Result<usize> {
    if inputs.is_empty() || inputs.len() > 1000 {
        bail!("Expected 1–1000 provider snapshots");
    }
    let mut payloads = Vec::new();
    let mut archives = Vec::new();
    let mut keys = HashSet::new();
    let mut total = 0;
    for input in inputs {
        let size = std::fs::metadata(input)?.len();
        total += size;
        if total > MAX_BYTES {
            bail!("Reference inputs exceed 32 MiB");
        }
        let payload = std::fs::read_to_string(input)?;
        let value: Value = serde_json::from_str(&payload)?;
        if value["schema"] == "a2m-provider-observations-v1" {
            let archive = serde_json::from_value::<ProviderArchive>(value)?.validate()?;
            if archives
                .iter()
                .any(|(source, _)| source == &archive.source.id)
            {
                bail!("Duplicate provider archive");
            }
            archives.push((archive.source.id.clone(), serde_json::to_string(&archive)?));
            continue;
        }
        let snapshot = decode(&payload).with_context(|| input.display().to_string())?;
        if !keys.insert((snapshot.source.id.clone(), snapshot.balance.id.clone())) {
            bail!("Duplicate source/balance period");
        }
        payloads.push((snapshot.source.id, snapshot.balance.id, payload));
    }
    payloads.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    archives.sort_by(|a, b| a.0.cmp(&b.0));
    // Reserve a new file; never overwrite a reference release or a user's history.
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    let result = (|| -> Result<()> {
        let mut conn = Connection::open(output)?;
        conn.pragma_update(None, "application_id", APPLICATION_ID)?;
        conn.pragma_update(None, "user_version", 2)?;
        let tx = conn.transaction()?;
        tx.execute_batch(
            "CREATE TABLE snapshots (
            source_id TEXT NOT NULL, balance_id TEXT NOT NULL, payload TEXT NOT NULL,
            PRIMARY KEY (source_id, balance_id));
            CREATE TABLE archives (source_id TEXT PRIMARY KEY, payload TEXT NOT NULL);
            CREATE VIEW observations AS SELECT a.source_id,
              CAST(r.key AS INTEGER) AS row_id,
              json_extract(r.value, '$.class_key') AS class_key,
              json_extract(r.value, '$.region') AS region,
              json_extract(r.value, '$.dungeon_id') AS dungeon_id,
              json_extract(r.value, '$.mob_code') AS mob_code,
              json_extract(r.value, '$.cp_min') AS cp_min,
              json_extract(r.value, '$.cp_max') AS cp_max,
              json_extract(r.value, '$.median_dps') AS median_dps,
              json_extract(r.value, '$.samples') AS samples,
              json_extract(r.value, '$.metric') AS metric,
              json_extract(r.value, '$.scope') AS scope,
              json_extract(r.value, '$.statistics') AS statistics,
              json_extract(r.value, '$.compatibility') AS compatibility
            FROM archives a, json_each(a.payload, '$.rows') r;",
        )?;
        for (source, balance, payload) in &payloads {
            tx.execute(
                "INSERT INTO snapshots VALUES (?1, ?2, ?3)",
                params![source, balance, payload],
            )?;
        }
        for (source, payload) in &archives {
            tx.execute(
                "INSERT INTO archives VALUES (?1, ?2)",
                params![source, payload],
            )?;
        }
        tx.commit()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(output);
    }
    result?;
    Ok(payloads.len() + archives.len())
}

pub fn read(path: &Path) -> Result<Bundle> {
    if std::fs::metadata(path)?.len() > MAX_BYTES {
        bail!("Reference database exceeds 32 MiB");
    }
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    read_connection(&conn)
}

/// The database travels with the binary, including standalone and packaged installs.
pub fn read_bundled() -> Result<Bundle> {
    let conn = bundled_connection()?;
    read_connection(&conn)
}

fn bundled_connection() -> Result<Connection> {
    let mut conn = Connection::open_in_memory()?;
    conn.deserialize_bytes(
        rusqlite::MAIN_DB,
        include_bytes!("../data/community/community-references.sqlite"),
    )?;
    Ok(conn)
}

fn read_connection(conn: &Connection) -> Result<Bundle> {
    let app: i64 = conn.pragma_query_value(None, "application_id", |r| r.get(0))?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if app != APPLICATION_ID || ![1, 2].contains(&version) {
        bail!("Unsupported reference database");
    }
    let mut stmt = conn.prepare("SELECT source_id, balance_id, payload FROM snapshots ORDER BY source_id, balance_id LIMIT 1001")?;
    let mut snapshots = Vec::new();
    let mut total = 0;
    let mut keys = HashSet::new();
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let source: String = row.get(0)?;
        let balance: String = row.get(1)?;
        let payload: String = row.get(2)?;
        total += payload.len() as u64;
        if total > MAX_BYTES || snapshots.len() == 1000 {
            bail!("Reference database exceeds snapshot limits");
        }
        let snapshot = decode(&payload)?;
        if snapshot.source.id != source
            || snapshot.balance.id != balance
            || !keys.insert((source, balance))
        {
            bail!("Reference provenance mismatch or duplicate");
        }
        snapshots.push(snapshot);
    }
    let mut archives = Vec::new();
    if version == 2 {
        let mut stmt =
            conn.prepare("SELECT source_id, payload FROM archives ORDER BY source_id LIMIT 8")?;
        let mut rows = stmt.query([])?;
        let mut archive_keys = HashSet::new();
        while let Some(row) = rows.next()? {
            let source: String = row.get(0)?;
            let payload: String = row.get(1)?;
            total += payload.len() as u64;
            if total > MAX_BYTES || archives.len() == 7 {
                bail!("Provider archive limit exceeded");
            }
            let archive = serde_json::from_str::<ProviderArchive>(&payload)?.validate()?;
            if source != archive.source.id || !archive_keys.insert(source) {
                bail!("Archive provenance mismatch or duplicate");
            }
            archives.push(archive);
        }
    }
    if snapshots.is_empty() && archives.is_empty() {
        bail!("Reference database is empty");
    }
    Ok(Bundle {
        snapshots,
        archives,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn embedded_database_is_read_only_and_keeps_weekly_cohorts_distinct() {
        let conn = bundled_connection().unwrap();
        assert!(conn.execute("DELETE FROM archives", []).is_err());
        let bundle = read_bundled().unwrap();
        assert_eq!(bundle.archives.len(), 6);
        let notmeter = bundle.archives.iter().find(|a| a.source.id == "notmeter").unwrap();
        assert_eq!(notmeter.rows.len(), 9133);
        let identities: HashSet<_> = notmeter.rows.iter().map(|r|
            serde_json::to_string(&serde_json::json!([
                r.class_key, r.scope["dungeon_key"], r.scope["boss_index"],
                r.scope["cp_tier"]["index"], r.scope["period"],
                r.scope["period_label"], r.scope["generated_at"]
            ])).unwrap()).collect();
        assert_eq!(identities.len(), notmeter.rows.len());
        assert_eq!(notmeter.rows.iter().filter(|r| r.scope["period_kind"] == "weekly").count(), 2232);
    }

    #[test]
    fn shipped_provider_database_has_six_sources_and_normalized_observations() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("data/community/community-references.sqlite");
        let bundle = read(&path).unwrap();
        assert!(bundle.snapshots.is_empty());
        assert_eq!(bundle.archives.len(), 6);
        assert_eq!(
            bundle.archives.iter().map(|a| a.rows.len()).sum::<usize>(),
            9272
        );
        let a2 = bundle
            .archives
            .iter()
            .find(|a| a.source.id == "a2tools")
            .unwrap();
        let scoped = a2
            .rows
            .iter()
            .find(|r| r.region.as_deref() == Some("EU"))
            .unwrap();
        assert_eq!(scoped.dungeon_id, Some(600072));
        assert_eq!(scoped.mob_code, Some(2300812));
        assert_eq!(scoped.cp_min, Some(71000));
        let quest = bundle
            .archives
            .iter()
            .find(|a| a.source.id == "questlog")
            .unwrap();
        assert!(
            quest
                .rows
                .iter()
                .all(|r| r.metric == "boss_dps_index" && r.median_dps.is_none())
        );
        let not = bundle
            .archives
            .iter()
            .find(|a| a.source.id == "notmeter")
            .unwrap();
        let bounded = not
            .rows
            .iter()
            .find(|r| r.scope["cp_tier"]["maxCombatPowerExclusive"].is_number())
            .unwrap();
        assert_eq!(
            bounded.cp_max.unwrap() + 1,
            bounded.scope["cp_tier"]["maxCombatPowerExclusive"]
                .as_i64()
                .unwrap()
        );
        let db = crate::db::Db::in_memory().unwrap();
        db.import_provider_archives(&bundle.archives).unwrap();
        // Idempotent reload, paged access, and no automatic promotion to scores.
        db.import_provider_archives(&bundle.archives).unwrap();
        let sources = db.community_sources().unwrap();
        assert_eq!(sources["offline_data"].as_array().unwrap().len(), 6);
        assert!(sources["imports"].as_array().unwrap().is_empty());
        let page = db.provider_observations("notmeter", 1, 2).unwrap().unwrap();
        assert_eq!(page["row_count"], 9133);
        assert_eq!(page["rows"].as_array().unwrap().len(), 2);
        assert_eq!(page["score_eligible"], false);
        assert!(db.provider_observations("missing", 0, 2).unwrap().is_none());
    }

    #[test]
    fn normalized_archive_rejects_false_class_and_non_numeric_median() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("data/community/providers/a2tools.json");
        let mut data: Value =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        data["rows"][0]["class_key"] = json!("invented-class");
        assert!(
            serde_json::from_value::<ProviderArchive>(data.clone())
                .unwrap()
                .validate()
                .is_err()
        );
        data["rows"][0]["class_key"] = json!("chanter");
        data["rows"][0]["median_dps"] = json!(-1);
        assert!(
            serde_json::from_value::<ProviderArchive>(data)
                .unwrap()
                .validate()
                .is_err()
        );
    }

    #[test]
    fn bundle_roundtrip_validation_and_history_isolation() {
        let dir = std::env::temp_dir().join(format!(
            "a2m-reference-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        let input = dir.join("provider.json");
        let output = dir.join("references.sqlite");
        let data = json!({
            "schema": "a2m-community-v2",
            "source": {"id":"a2tools", "url":"https://a2tools.app/stats", "captured_at":1790000000000_i64, "rights_confirmed":true},
            "balance": {"id":"test-patch", "from_ms":1780000000000_i64, "until_ms":1790000000000_i64},
            "methodology": {"metric":"fight_dps", "outcome":"confirmed_kill", "aggregation":"median_unique_players", "patch_id":"test-patch"},
            "rows":[{"region":"KR", "dungeon_id":600093, "mob_code":2300409, "class_key":"gladiator", "cp_min":60000, "cp_max":80000, "median_dps":12345.0, "samples":10}]
        });
        std::fs::write(&input, data.to_string()).unwrap();
        assert_eq!(build(&output, std::slice::from_ref(&input)).unwrap(), 1);
        assert!(build(&output, std::slice::from_ref(&input)).is_err());
        let conn = Connection::open(&output).unwrap();
        let fights: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='fights'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(fights, 0);
        let db = crate::db::Db::in_memory().unwrap();
        for snapshot in read(&output).unwrap().snapshots {
            db.import_community(snapshot).unwrap();
        }
        let sources = db.community_sources().unwrap();
        assert_eq!(sources["imports"][0]["source_id"], "a2tools");
        assert_eq!(sources["imports"][0]["rows"], 1);
        // A tampered bundle must fail before the application imports anything.
        conn.execute("UPDATE snapshots SET balance_id='tampered'", [])
            .unwrap();
        assert!(read(&output).is_err());
        drop(conn);
        let mut invalid = data;
        invalid["rows"][0]["median_dps"] = json!(-1);
        std::fs::write(&input, invalid.to_string()).unwrap();
        let bad_output = dir.join("invalid.sqlite");
        assert!(build(&bad_output, &[input]).is_err());
        assert!(!bad_output.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
