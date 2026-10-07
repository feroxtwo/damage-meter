//! Offline display metadata. Exact IDs and audited aliases never change combat totals.
use serde_json::{Value, json};
use std::{collections::HashMap, sync::LazyLock};

pub static CATALOG: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../data/skills/catalog.json"))
        .expect("bundled skill catalog")
});
static BY_ID: LazyLock<HashMap<i64, &'static Value>> = LazyLock::new(|| {
    let mut result = HashMap::new();
    for skill in CATALOG["skills"].as_array().expect("skill entries") {
        result.insert(skill["id"].as_i64().unwrap(), skill);
        for alias in skill["aliases"].as_array().unwrap() {
            result.insert(alias.as_i64().unwrap(), skill);
        }
    }
    result
});
static DE: LazyLock<HashMap<String, String>> =
    LazyLock::new(|| serde_json::from_str(include_str!("../data/i18n/skills/de.json")).unwrap());
static EN: LazyLock<HashMap<String, String>> =
    LazyLock::new(|| serde_json::from_str(include_str!("../data/i18n/skills/en.json")).unwrap());
static ICONS: LazyLock<Value> =
    LazyLock::new(|| serde_json::from_str(include_str!("../data/skills/icons.json")).unwrap());
static PACK: &[u8] = include_bytes!("../data/skills/icons.bin");
pub const CLASS_KEYS: [&str; 9] = [
    "gladiator",
    "templar",
    "assassin",
    "ranger",
    "sorcerer",
    "elementalist",
    "cleric",
    "chanter",
    "fighter",
];
pub fn icon(name: &str) -> Option<&'static [u8]> {
    let entry = ICONS.get(name)?;
    let start = usize::try_from(entry["offset"].as_u64()?).ok()?;
    let length = usize::try_from(entry["length"].as_u64()?).ok()?;
    PACK.get(start..start.checked_add(length)?)
}
pub fn class_rgba(key: &str) -> Option<&'static [u8]> {
    let index = CLASS_KEYS.iter().position(|c| *c == key)?;
    include_bytes!("../data/skills/classes.rgba").get(index * 4096..(index + 1) * 4096)
}
fn valid_name(name: Option<&String>) -> Option<&str> {
    name.filter(|s| !s.is_empty() && !s.contains("???"))
        .map(String::as_str)
}
pub fn metadata(code: i64) -> Option<Value> {
    if let Some(skill) = BY_ID.get(&code) {
        return Some(
            json!({"names":skill["names"],"icon":skill["icon"],"skill_id":skill["id"],"name_source_de":skill["name_source_de"]}),
        );
    }
    let key = code.to_string();
    let de = valid_name(DE.get(&key));
    let en = valid_name(EN.get(&key));
    if de.is_none() && en.is_none() {
        return None;
    }
    Some(json!({"names":{"de":de,"en":en},"icon":null}))
}
/// Enrich only skill/effect display rows. Raw names stay available in storage;
/// recursively applying metadata to a saved report leaves every number intact.
pub fn decorate(value: &mut Value, language: &str) {
    match value {
        Value::Array(items) => {
            for item in items {
                decorate(item, language);
            }
        }
        Value::Object(fields) => {
            for item in fields.values_mut() {
                decorate(item, language);
            }
            if fields.contains_key("name")
                && let Some(code) = fields.get("code").and_then(Value::as_i64)
                && let Some(Value::Object(meta)) = metadata(code)
            {
                if let Some(name) = meta
                    .get("names")
                    .and_then(|n| n.get(language))
                    .and_then(Value::as_str)
                {
                    fields.insert("name".into(), json!(name));
                }
                fields.extend(meta);
            }
            if let Some(key) = fields
                .get("class_key")
                .and_then(Value::as_str)
                .map(str::to_owned)
                && CLASS_KEYS.contains(&key.as_str())
            {
                fields.insert(
                    "class_icon".into(),
                    json!(format!("/assets/icons/class-{key}.webp")),
                );
            }
        }
        _ => {}
    }
}
/// All existing packet IDs are searchable; primary entries retain their class/aliases.
pub fn all() -> Value {
    let mut value = CATALOG.clone();
    value["variants"] = Value::Array(
        EN.keys()
            .filter_map(|id| {
                let code: i64 = id.parse().ok()?;
                if BY_ID.contains_key(&code) {
                    return None;
                }
                let mut row = metadata(code)?;
                row["id"] = json!(code);
                row["class_key"] = json!("unknown");
                Some(row)
            })
            .collect(),
    );
    value
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_assets_and_aliases_are_valid() {
        assert_eq!(CATALOG["skills"].as_array().unwrap().len(), 364);
        let mut ids = std::collections::HashSet::new();
        for s in CATALOG["skills"].as_array().unwrap() {
            assert!(ids.insert(s["id"].as_i64().unwrap()));
            for lang in ["de", "en"] {
                assert!(!s["names"][lang].as_str().unwrap().is_empty());
            }
            if let Some(path) = s["icon"].as_str() {
                assert!(
                    icon(path.trim_start_matches("/assets/icons/"))
                        .unwrap()
                        .starts_with(b"RIFF")
                );
            }
        }
        for key in CLASS_KEYS {
            assert_eq!(class_rgba(key).unwrap().len(), 4096);
            assert!(icon(&format!("class-{key}.webp")).is_some());
        }
        assert!(icon("../icons.bin").is_none());
        assert!(metadata(-1).is_none());
        assert_eq!(metadata(11170000).unwrap()["names"]["en"], "Overhead Slam");
        assert_eq!(metadata(11170010).unwrap()["skill_id"], 11170000);
    }
    #[test]
    fn language_changes_saved_labels_only() {
        let mut report = json!({"players":[{"name":"Player","class_key":"gladiator","skills":[{"code":11170010,"name":"old","damage":12345,"hits":7,"dps":456.0,"hit_timestamps":[100,300]}]}]});
        decorate(&mut report, "en");
        assert_eq!(report["players"][0]["skills"][0]["name"], "Overhead Slam");
        decorate(&mut report, "de");
        let skill = &report["players"][0]["skills"][0];
        assert_eq!(skill["name"], "Abwärtsschlag");
        assert_eq!(skill["damage"], 12345);
        assert_eq!(skill["hits"], 7);
        assert_eq!(skill["dps"], 456.0);
        assert_eq!(skill["hit_timestamps"], json!([100, 300]));
        assert_eq!(report["players"][0]["name"], "Player");
    }
}
