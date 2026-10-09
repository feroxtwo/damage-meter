//! Strict schema for user-supplied, permissioned aggregate comparison snapshots.
//! Never fetch community websites or send fight/character data automatically.
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;

pub const SCHEMA: &str = "a2m-community-v1";

#[derive(Debug, Deserialize)]
pub struct CommunitySnapshot {
    pub schema: String,
    pub source: ReferenceSource,
    pub balance: BalancePeriod,
    pub rows: Vec<ReferenceRow>,
}

#[derive(Debug, Deserialize)]
pub struct ReferenceSource {
    pub id: String,
    pub url: String,
    pub captured_at: i64,
    pub rights_confirmed: bool,
}

#[derive(Debug, Deserialize)]
pub struct BalancePeriod {
    pub id: String,
    pub from_ms: i64,
    pub until_ms: i64,
}

#[derive(Debug, Deserialize)]
pub struct ReferenceRow {
    pub region: String,
    pub dungeon_id: i32,
    pub mob_code: i32,
    pub class_key: String,
    pub cp_min: i64,
    pub cp_max: i64,
    pub median_dps: f64,
    pub samples: u32,
}

pub fn valid_region(value: &str) -> bool {
    matches!(
        value,
        "ALL" | "EU" | "NAE" | "NAW" | "SA" | "ASIA" | "KR" | "TW"
    )
}

pub fn valid_class(value: &str) -> bool {
    matches!(
        value,
        "all"
            | "gladiator"
            | "templar"
            | "ranger"
            | "assassin"
            | "sorcerer"
            | "cleric"
            | "elementalist"
            | "chanter"
            | "fighter"
    )
}

impl CommunitySnapshot {
    /// A self-attestation only. The program cannot certify the uploader's rights
    /// nor that their JSON truly originated from the named third-party source.
    pub fn validate(self) -> Result<Self, &'static str> {
        if self.schema != SCHEMA || self.rows.is_empty() || self.rows.len() > 500 {
            return Err("invalid_schema_or_row_count");
        }
        if !self.source.rights_confirmed {
            return Err("rights_confirmation_required");
        }
        if !matches!(self.source.id.as_str(), "a2tools" | "aiondps" | "community") {
            return Err("unknown_source");
        }
        let url = reqwest::Url::parse(&self.source.url).map_err(|_| "invalid_source_url")?;
        let host = url.host_str().ok_or("invalid_source_url")?;
        if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
            return Err("invalid_source_url");
        }
        match self.source.id.as_str() {
            "a2tools" if host != "a2tools.app" && host != "www.a2tools.app" => {
                return Err("source_url_mismatch");
            }
            "aiondps" if host != "aiondps.com" && host != "www.aiondps.com" => {
                return Err("source_url_mismatch");
            }
            _ => {}
        }
        if !(1_600_000_000_000..=2_500_000_000_000).contains(&self.source.captured_at) {
            return Err("invalid_capture_time");
        }
        let period = &self.balance;
        if period.id.is_empty()
            || period.id.len() > 64
            || !period
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            || period.from_ms < 1_600_000_000_000
            || period.until_ms <= period.from_ms
            || period.until_ms - period.from_ms > 180 * 86_400_000
        {
            return Err("invalid_balance_period");
        }
        let mut keys = HashSet::new();
        for row in &self.rows {
            if !valid_region(&row.region)
                || !valid_class(&row.class_key)
                || row.dungeon_id < 0
                || row.mob_code <= 0
                || row.cp_min <= 0
                || row.cp_max < row.cp_min
                || row.cp_max - row.cp_min > 20_000
                || !row.median_dps.is_finite()
                || row.median_dps <= 0.0
                || row.median_dps > 1_000_000_000.0
                || !(5..=1_000_000).contains(&row.samples)
            {
                return Err("invalid_reference_row");
            }
            if !keys.insert((
                row.region.as_str(),
                row.dungeon_id,
                row.mob_code,
                row.class_key.as_str(),
                row.cp_min,
                row.cp_max,
            )) {
                return Err("duplicate_reference_row");
            }
        }
        Ok(self)
    }
}

/// Source capabilities are declared, not inferred from public pages.
/// A third-party upload endpoint is not necessarily a statistics download API.
pub fn provider_catalog() -> Value {
    json!([
        {
            "id": "local",
            "name": "Eigene Kampfaufzeichnungen",
            "kind": "community",
            "status": "active",
            "url": null,
            "detail": "Lokaler Skill Index, ohne Upload"
        },
        {
            "id": "game_catalog",
            "name": "Integrierte Spielmetadaten",
            "kind": "game_data",
            "status": "active",
            "url": null,
            "detail": "Vorhandene Skill-, NPC- und Instanzkataloge"
        },
        {
            "id": "a2tools",
            "name": "A2 Tools",
            "kind": "community",
            "status": "permission_required",
            "url": "https://a2tools.app/stats",
            "detail": "Community-Mediane öffentlich sichtbar; Datenexport/API nicht zugesagt"
        },
        {
            "id": "aiondps",
            "name": "Aion DPS",
            "kind": "community",
            "status": "api_unverified",
            "url": "https://aiondps.com",
            "detail": "Öffentliche Ranglisten-Routen im Quellcode, Datenabruf nicht verifiziert"
        },
        {
            "id": "ncsoft",
            "name": "AION 2 Charakterprofile",
            "kind": "character",
            "status": "not_connected",
            "url": "https://github.com/nuriland/aion2-api",
            "detail": "Inoffizielle Client-Bibliothek; kein ungefragter Charakterlookup"
        },
        {
            "id": "community",
            "name": "Berechtigter JSON-Datensatz",
            "kind": "community",
            "status": "import_ready",
            "url": null,
            "detail": "Aggregierte Referenzdaten lokal und freiwillig importieren"
        }
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> CommunitySnapshot {
        serde_json::from_value(json!({
            "schema": SCHEMA,
            "source": {
                "id": "community", "url": "https://example.org/statistics",
                "captured_at": 1_780_000_000_000_i64, "rights_confirmed": true
            },
            "balance": {
                "id": "oct-2026", "from_ms": 1_780_000_000_000_i64,
                "until_ms": 1_792_000_000_000_i64
            },
            "rows": [{
                "region": "EU", "dungeon_id": 600093, "mob_code": 2300409,
                "class_key": "gladiator", "cp_min": 60000, "cp_max": 80000,
                "median_dps": 14500.0, "samples": 120
            }]
        }))
        .unwrap()
    }

    #[test]
    fn validates_rights_scope_and_provenance() {
        assert!(snapshot().validate().is_ok());
        let mut wrong = snapshot();
        wrong.source.rights_confirmed = false;
        assert_eq!(
            wrong.validate().unwrap_err(),
            "rights_confirmation_required"
        );
        let mut wrong = snapshot();
        wrong.source.id = "a2tools".into();
        assert_eq!(wrong.validate().unwrap_err(), "source_url_mismatch");
        let mut wrong = snapshot();
        wrong.rows[0].median_dps = f64::NAN;
        assert_eq!(wrong.validate().unwrap_err(), "invalid_reference_row");
        let mut wrong = snapshot();
        wrong.rows[0].cp_max = 95_000;
        assert_eq!(wrong.validate().unwrap_err(), "invalid_reference_row");
        let mut wrong = snapshot();
        wrong.rows[0].samples = 4;
        assert_eq!(wrong.validate().unwrap_err(), "invalid_reference_row");
        let mut wrong = snapshot();
        wrong.rows.push(ReferenceRow {
            region: wrong.rows[0].region.clone(),
            dungeon_id: wrong.rows[0].dungeon_id,
            mob_code: wrong.rows[0].mob_code,
            class_key: wrong.rows[0].class_key.clone(),
            cp_min: wrong.rows[0].cp_min,
            cp_max: wrong.rows[0].cp_max,
            median_dps: 14500.0,
            samples: 120,
        });
        assert_eq!(wrong.validate().unwrap_err(), "duplicate_reference_row");
    }
}
