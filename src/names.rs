//! Display names: classes and dungeons.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ClassInfo {
    /// Stable key, used by the web UI for colours and icons.
    pub key: &'static str,
    pub name: &'static str,
    /// RGB bar colour.
    pub color: [u8; 3],
}

const UNKNOWN: ClassInfo = ClassInfo {
    key: "unknown",
    name: "",
    color: [120, 120, 130],
};

/// The parser names classes by their Korean name; map that to ours.
pub fn class_info(job: &str) -> ClassInfo {
    match job {
        "검성" => ClassInfo {
            key: "gladiator",
            name: "Gladiator",
            color: [196, 120, 64],
        },
        "수호성" => ClassInfo {
            key: "templar",
            name: "Templer",
            color: [214, 182, 92],
        },
        "궁성" => ClassInfo {
            key: "ranger",
            name: "Jäger",
            color: [120, 186, 84],
        },
        "살성" => ClassInfo {
            key: "assassin",
            name: "Assassine",
            color: [200, 90, 200],
        },
        "마도성" => ClassInfo {
            key: "sorcerer",
            name: "Magier",
            color: [88, 150, 236],
        },
        "치유성" => ClassInfo {
            key: "cleric",
            name: "Kleriker",
            color: [236, 236, 236],
        },
        "정령성" => ClassInfo {
            key: "elementalist",
            name: "Beschwörer",
            color: [150, 110, 230],
        },
        "호법성" => ClassInfo {
            key: "chanter",
            name: "Kantor",
            color: [90, 210, 200],
        },
        "권성" => ClassInfo {
            key: "fighter",
            name: "Faustkämpfer",
            color: [220, 80, 80],
        },
        _ => UNKNOWN,
    }
}

#[derive(serde::Deserialize)]
struct DungeonEntry {
    name: String,
    #[serde(default)]
    difficulty: Option<String>,
    #[serde(default)]
    activity: Option<String>,
}

static DUNGEONS: LazyLock<HashMap<i32, DungeonEntry>> = LazyLock::new(|| {
    let raw: HashMap<String, DungeonEntry> =
        serde_json::from_str(include_str!("../data/i18n/dungeons/en.json")).unwrap_or_default();
    raw.into_iter()
        .filter_map(|(k, v)| Some((k.parse().ok()?, v)))
        .collect()
});

pub fn dungeon_name(id: i32) -> Option<String> {
    DUNGEONS.get(&id).map(|d| d.name.clone())
}

/// Only catalogued instances have a verified activity; unknown IDs stay unknown.
pub fn dungeon_activity(id: i32) -> Option<&'static str> {
    DUNGEONS.get(&id)?.activity.as_deref()
}

// Index once: live ticks and history queries must not rescan the NPC catalog.
static BOSS_METADATA: LazyLock<HashMap<i32, (Vec<i32>, HashSet<String>)>> = LazyLock::new(|| {
    let catalog: Value = serde_json::from_str(include_str!("../data/i18n/npcs/en.json"))
        .expect("NPC catalog");
    let mut index: HashMap<i32, (Vec<i32>, HashSet<String>)> = HashMap::new();
    for (code, npc) in catalog.as_object().unwrap() {
        if npc["isBoss"] != true { continue; }
        let (Some(id), Ok(code)) = (npc["dungeonId"].as_i64(), code.parse::<i32>()) else { continue; };
        let entry = index.entry(id as i32).or_default();
        entry.0.push(code);
        if let Some(tier) = npc["tier"].as_str() { entry.1.insert(tier.to_string()); }
    }
    index
});

pub fn dungeon_bosses(id: i32) -> Vec<i32> {
    BOSS_METADATA.get(&id).map(|entry| entry.0.clone()).unwrap_or_default()
}

/// Use explicit NPC tiers for transcendence; map ID suffixes are not stages.
pub fn dungeon_difficulty(id: i32) -> Option<String> {
    let entry = DUNGEONS.get(&id)?;
    let n = id % 10;
    if entry.activity.as_deref() == Some("transcendence") {
        let tiers = &BOSS_METADATA.get(&id)?.1;
        return (tiers.len() == 1)
            .then(|| tiers.iter().next().unwrap().replace("Stage ", "Stufe "));
    }
    let key = entry.difficulty.clone().or_else(|| {
        match n {
            1 => Some("exploration"),
            2 => Some("normal"),
            3 => Some("hard"),
            _ => None,
        }
        .map(str::to_string)
    })?;
    Some(
        match key.as_str() {
            "exploration" => "Erkundung",
            "normal" => "Normal",
            "hard" => "Schwer",
            other => other,
        }
        .to_string(),
    )
}

/// "Ferocious Horn Den (Schwer)", or "Instanz 600099" for one we do not know.
pub fn dungeon_label(id: i32) -> String {
    if id <= 0 {
        return "Offene Welt".into();
    }
    match (dungeon_name(id), dungeon_difficulty(id)) {
        (Some(name), Some(tier)) => format!("{name} ({tier})"),
        (Some(name), None) => name,
        _ => format!("Instanz {id}"),
    }
}

/// 1234567 -> "1,23M", 45600 -> "45,6K".
pub fn short_number(v: f64) -> String {
    let (n, suffix) = if v >= 1e9 {
        (v / 1e9, "B")
    } else if v >= 1e6 {
        (v / 1e6, "M")
    } else if v >= 1e3 {
        (v / 1e3, "K")
    } else {
        return format!("{v:.0}");
    };
    let digits = if n >= 100.0 {
        0
    } else if n >= 10.0 {
        1
    } else {
        2
    };
    format!("{n:.digits$}{suffix}").replace('.', ",")
}

pub fn duration(ms: i64) -> String {
    let s = (ms / 1000).max(0);
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dungeon_labels() {
        assert_eq!(dungeon_label(600093), "Ferocious Horn Den (Schwer)");
        assert_eq!(dungeon_label(600001), "Krao Cave (Erkundung)");
        assert_eq!(dungeon_label(600055), "Deus Research Base (Stufe 2)");
        assert_eq!(dungeon_difficulty(600063).as_deref(), Some("Stufe 1"));
        assert_eq!(dungeon_bosses(600063).len(), 3);
        assert_eq!(dungeon_label(0), "Offene Welt");
        assert_eq!(dungeon_label(123), "Instanz 123");
        assert_eq!(dungeon_activity(600093), Some("expedition"));
        assert_eq!(dungeon_activity(600055), Some("transcendence"));
        assert_eq!(dungeon_activity(600163), Some("expedition"));
        assert_eq!(dungeon_activity(600999), None);
    }

    #[test]
    fn numbers() {
        assert_eq!(short_number(1_234_567.0), "1,23M");
        assert_eq!(short_number(45_600.0), "45,6K");
        assert_eq!(short_number(999.0), "999");
        assert_eq!(duration(65_000), "1:05");
        assert_eq!(duration(3_725_000), "1:02:05");
    }

    #[test]
    fn classes() {
        assert_eq!(class_info("치유성").name, "Kleriker");
        assert_eq!(class_info("???").key, "unknown");
    }
}
