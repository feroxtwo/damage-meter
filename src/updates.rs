//! Opt-in release lookup. No startup requests, telemetry or downloaded executables.
use serde_json::{Value, json};
const RELEASES: &str = "https://github.com/feroxtwo/damage-meter/releases";
pub const PARSER_REV: &str = "d3cf6f92533721939f4b4454d163c6d1dd820666";
fn version(s: &str) -> Option<(u64, u64, u64)> {
    let values: Vec<_> = s
        .trim_start_matches('v')
        .split('.')
        .map(str::parse::<u64>)
        .collect();
    if values.len() != 3 {
        return None;
    }
    Some((
        *values[0].as_ref().ok()?,
        *values[1].as_ref().ok()?,
        *values[2].as_ref().ok()?,
    ))
}
pub fn info() -> Value {
    json!({"version":env!("CARGO_PKG_VERSION"),"parser_version":"2.0.52","parser_rev":PARSER_REV})
}
pub fn release_status(tag: &str) -> Value {
    let newer = version(tag)
        .zip(version(env!("CARGO_PKG_VERSION")))
        .is_some_and(|(a, b)| a > b);
    let message = if version(tag).is_none() {
        "Veröffentlichung ohne auswertbare Versionsnummer.".into()
    } else if newer {
        format!(
            "Update {tag} verfügbar. Einstellungen und Kämpfe bleiben beim Paketupdate erhalten."
        )
    } else {
        format!(
            "Installiert: {}. Letzte Veröffentlichung: {tag}. Kein neueres Release gefunden.",
            env!("CARGO_PKG_VERSION")
        )
    };
    json!({"available":newer,"message":message,"url":RELEASES})
}
pub async fn check() -> anyhow::Result<Value> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("aion2-meter-release-check")
        .build()?;
    let mut response = client
        .get("https://api.github.com/repos/feroxtwo/damage-meter/releases/latest")
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(
            json!({"available":false,"message":"Noch kein öffentliches Release vorhanden. Testpakete liegen in GitHub Actions.","url":RELEASES}),
        );
    }
    response.error_for_status_ref()?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        anyhow::ensure!(
            bytes.len() + chunk.len() <= 65_536,
            "Release response too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    let data: Value = serde_json::from_slice(&bytes)?;
    Ok(release_status(data["tag_name"].as_str().unwrap_or("")))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn release_comparison_is_numeric_and_never_links_to_remote_html_url() {
        assert!(release_status("v0.10.0")["available"].as_bool().unwrap());
        assert!(!release_status("v0.2.99")["available"].as_bool().unwrap());
        assert!(!release_status("bad")["available"].as_bool().unwrap());
        assert_eq!(release_status("v1.0.0")["url"], RELEASES);
    }
}
