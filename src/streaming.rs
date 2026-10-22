//! Dedicated read-only OBS listener. Never exposes the dashboard or raw evidence.
use crate::engine::Engine;
use axum::{
    Json, Router,
    extract::{ConnectInfo, Path, Request, State},
    http::{StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use parking_lot::RwLock;
use serde_json::{Value, json};
use std::{
    io::Read,
    net::{IpAddr, SocketAddr},
    sync::Arc,
};
pub struct Access {
    pub key: RwLock<String>,
    pub address: RwLock<Option<SocketAddr>>,
    slots: Arc<tokio::sync::Semaphore>,
}
impl Default for Access {
    fn default() -> Self {
        Self {
            key: RwLock::new(String::new()),
            address: RwLock::new(None),
            slots: Arc::new(tokio::sync::Semaphore::new(8)),
        }
    }
}
pub fn new_key() -> std::io::Result<String> {
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}
impl Access {
    pub fn rotate(&self) -> anyhow::Result<()> {
        *self.key.write() = new_key()?;
        Ok(())
    }
    fn valid(&self, key: &str) -> bool {
        let expected = self.key.read();
        if key.len() != 64 || expected.len() != 64 {
            return false;
        }
        key.bytes()
            .zip(expected.bytes())
            .fold(0, |acc, (a, b)| acc | (a ^ b))
            == 0
    }
}
fn private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_private() || ip.is_loopback() || ip.is_link_local(),
        IpAddr::V6(ip) => ip
            .to_ipv4_mapped()
            .map(|v| private(v.into()))
            .unwrap_or(ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local()),
    }
}
async fn restricted(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    State((addr, e)): State<(SocketAddr, Arc<Engine>)>,
    request: Request,
    next: Next,
) -> Response {
    let Ok(_permit) = e.stream.slots.clone().try_acquire_owned() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let host_ok = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<axum::http::uri::Authority>().ok())
        .is_some_and(|h| {
            h.port_u16().unwrap_or(80) == addr.port()
                && (h.host() == "localhost"
                    || h.host()
                        .trim_matches(['[', ']'])
                        .parse::<IpAddr>()
                        .is_ok_and(private))
        });
    if !private(peer.ip()) || !host_ok {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut response = next.run(request).await;
    for (name, value) in [
        ("cache-control", "no-store"),
        ("referrer-policy", "no-referrer"),
        ("x-content-type-options", "nosniff"),
        ("x-frame-options", "DENY"),
        (
            "content-security-policy",
            "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; img-src 'self'; frame-ancestors 'none'",
        ),
    ] {
        response.headers_mut().insert(
            axum::http::HeaderName::from_static(name),
            axum::http::HeaderValue::from_static(value),
        );
    }
    response
}
fn authorize(e: &Engine, key: &str) -> Result<(), StatusCode> {
    if !e.overlay.read().stream_enabled || !e.stream.valid(key) {
        Err(StatusCode::NOT_FOUND)
    } else {
        Ok(())
    }
}
async fn page(
    State(e): State<Arc<Engine>>,
    Path(key): Path<String>,
) -> Result<Html<String>, StatusCode> {
    authorize(&e, &key)?;
    Ok(Html(
        include_str!("../web/overlay.html")
            .replace("'/api/live'", "'./live'")
            .replace("/assets/icons/", "./icons/"),
    ))
}
async fn live(
    State(e): State<Arc<Engine>>,
    Path(key): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    authorize(&e, &key)?;
    let mut l = serde_json::to_value(e.live()).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let names = e.overlay.read().stream_names;
    let mut rows = l["rows"].take();
    if let Some(rows) = rows.as_array_mut() {
        for row in rows {
            if !names {
                row["name"] = row["class_name"].clone();
            }
            if let Some(row) = row.as_object_mut() {
                row.remove("id");
                row.remove("combat_power");
            }
        }
    }
    let mut overlay = l["overlay"].take();
    if let Some(o) = overlay.as_object_mut() {
        o.remove("position");
        o.remove("cooldown_skills");
        o.remove("stream_names");
    }
    Ok(Json(
        json!({"rows":rows,"overlay":overlay,"target_name":l["target_name"],"battle_time_ms":l["battle_time_ms"],"target_hp":l["target_hp"],"hp_estimated":l["hp_estimated"],"combat_state":l["combat_state"],"numeric_limited":l["numeric_limited"],"ping_ms":l["ping_ms"],"capture":{"permission":l["capture"]["permission"],"game_running":l["capture"]["game_running"],"locked_port":if l["capture"]["locked_port"].is_null(){Value::Null}else{json!(1)},"error":if l["capture"]["error"].is_null(){Value::Null}else{json!("Mitschnitt gestoppt")}},"cooldowns":[],"boss_cast":null}),
    ))
}
async fn icon(State(e): State<Arc<Engine>>, Path((key, name)): Path<(String, String)>) -> Response {
    if authorize(&e, &key).is_err() {
        return StatusCode::NOT_FOUND.into_response();
    }
    match crate::skills::icon(&name) {
        Some(bytes) => ([(header::CONTENT_TYPE, "image/webp")], bytes).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
pub fn router(e: Arc<Engine>, addr: SocketAddr) -> Router {
    Router::new()
        .route("/s/{key}/", get(page))
        .route("/s/{key}/live", get(live))
        .route("/s/{key}/icons/{name}", get(icon))
        .with_state(e.clone())
        .layer(middleware::from_fn_with_state((addr, e), restricted))
}
pub async fn serve(e: Arc<Engine>, listener: std::net::TcpListener) -> anyhow::Result<()> {
    let addr = listener.local_addr()?;
    let listener = tokio::net::TcpListener::from_std(listener)?;
    axum::serve(
        listener,
        router(e.clone(), addr).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        while !crate::engine::Engine::is_stopping(&e) {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    })
    .await?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use tower::ServiceExt;
    #[test]
    fn key_and_peer_rules() {
        let a = Access::default();
        a.rotate().unwrap();
        let first = a.key.read().clone();
        assert_eq!(first.len(), 64);
        assert!(a.valid(&first));
        a.rotate().unwrap();
        assert!(!a.valid(&first));
        assert!(!private("8.8.8.8".parse().unwrap()));
        assert!(private("192.168.1.2".parse().unwrap()));
    }
    #[tokio::test]
    async fn only_authorized_read_only_snapshot_is_exposed() {
        let e = Engine::new(
            crate::db::Db::in_memory().unwrap(),
            "en",
            std::env::temp_dir(),
        );
        e.stream.rotate().unwrap();
        e.modify_overlay(|s| s.stream_enabled = true).unwrap();
        let key = e.stream.key.read().clone();
        let app = router(e, "127.0.0.1:8788".parse().unwrap());
        for (path, method, status) in [
            (format!("/s/{key}/live"), "GET", 200),
            ("/api/live".into(), "GET", 404),
            (format!("/s/{key}/live"), "POST", 405),
            ("/s/wrong/live".into(), "GET", 404),
        ] {
            let mut req = Request::builder()
                .uri(path)
                .method(method)
                .header("host", "127.0.0.1:8788")
                .body(Body::empty())
                .unwrap();
            req.extensions_mut().insert(ConnectInfo(
                "127.0.0.1:10000".parse::<SocketAddr>().unwrap(),
            ));
            let response = app.clone().oneshot(req).await.unwrap();
            assert_eq!(response.status().as_u16(), status);
        }
    }
}
