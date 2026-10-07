//! The local web app: live meter, run history and statistics.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Path, Query, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::engine::{Engine, OverlaySettings};

/// Mutating requests must carry this header. A web page can only send a
/// custom header to another origin after a CORS preflight, which this server
/// never approves, so no website can delete runs through your browser.
pub const ACTION_HEADER: &str = "x-a2m";

type AppState = Arc<Engine>;

fn guard(headers: &HeaderMap) -> Result<(), StatusCode> {
    if headers.contains_key(ACTION_HEADER) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

fn db_error(e: anyhow::Error) -> StatusCode {
    tracing::error!("Database: {e:#}");
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../web/index.html"))
}

async fn overlay_page() -> Html<&'static str> {
    Html(include_str!("../web/overlay.html"))
}

async fn live(State(engine): State<AppState>) -> Response {
    ([(header::CACHE_CONTROL, "no-store")], Json(engine.live())).into_response()
}

#[derive(Deserialize)]
struct RunsQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    dungeon: Option<String>,
    character: Option<String>,
}

async fn runs(
    State(engine): State<AppState>,
    Query(q): Query<RunsQuery>,
) -> Result<Json<Value>, StatusCode> {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let offset = q.offset.unwrap_or(0).max(0);
    let dungeon = q.dungeon.as_deref().filter(|d| !d.is_empty());
    let character = q.character.as_deref().unwrap_or("");
    engine
        .db
        .list_runs(limit, offset, dungeon, character)
        .map(Json)
        .map_err(db_error)
}

async fn run_detail(
    State(engine): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, StatusCode> {
    engine
        .db
        .run_detail(id)
        .map_err(db_error)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn delete_run(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<StatusCode, StatusCode> {
    guard(&headers)?;
    engine.db.delete_run(id).map_err(db_error)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct NoteBody {
    note: String,
}

async fn run_note(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<NoteBody>,
) -> Result<StatusCode, StatusCode> {
    guard(&headers)?;
    engine
        .db
        .set_run_note(id, body.note.trim())
        .map_err(db_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn fight_detail(
    State(engine): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    engine
        .db
        .fight_detail(&id)
        .map_err(db_error)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// `?character=Name` limits a statistic to one of your characters.
#[derive(Deserialize)]
struct StatsQuery {
    limit: Option<i64>,
    #[serde(default)]
    character: String,
}

async fn partners(
    State(engine): State<AppState>,
    Query(q): Query<StatsQuery>,
) -> Result<Json<Value>, StatusCode> {
    let rows = engine
        .db
        .top_partners(q.limit.unwrap_or(5).clamp(1, 100), &q.character)
        .map_err(db_error)?;
    Ok(Json(Value::Array(rows)))
}

async fn boss_history(
    State(engine): State<AppState>,
    Query(q): Query<StatsQuery>,
) -> Result<Json<Value>, StatusCode> {
    engine
        .db
        .boss_history(&q.character)
        .map(Json)
        .map_err(db_error)
}

async fn summary(
    State(engine): State<AppState>,
    Query(q): Query<StatsQuery>,
) -> Result<Json<Value>, StatusCode> {
    engine.db.summary(&q.character).map(Json).map_err(db_error)
}

async fn characters(State(engine): State<AppState>) -> Result<Json<Value>, StatusCode> {
    engine
        .db
        .characters()
        .map(|c| Json(Value::Array(c)))
        .map_err(db_error)
}

async fn get_overlay(State(engine): State<AppState>) -> Json<OverlaySettings> {
    Json(engine.overlay.read().clone())
}

async fn set_overlay(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<OverlaySettings>,
) -> Result<Json<OverlaySettings>, StatusCode> {
    guard(&headers)?;
    engine.update_overlay(body).map(Json).map_err(db_error)
}

async fn toggle_lock(
    State(engine): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    guard(&headers)?;
    let s = engine
        .modify_overlay(|s| s.locked = !s.locked)
        .map_err(db_error)?;
    Ok(Json(json!({ "locked": s.locked })))
}

async fn toggle_visible(
    State(engine): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    guard(&headers)?;
    let s = engine
        .modify_overlay(|s| s.visible = !s.visible)
        .map_err(db_error)?;
    Ok(Json(json!({ "visible": s.visible })))
}

async fn reset(
    State(engine): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    guard(&headers)?;
    engine.request_reset();
    Ok(StatusCode::NO_CONTENT)
}

async fn toggle_record(
    State(engine): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    guard(&headers)?;
    let on = engine.toggle_recording();
    let dir = engine.capture_dir().display().to_string();
    Ok(Json(json!({ "recording": on, "dir": dir })))
}

#[derive(Deserialize)]
struct RecordBody {
    on: bool,
}

async fn set_record(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<RecordBody>,
) -> Result<Json<Value>, StatusCode> {
    guard(&headers)?;
    engine.set_recording(body.on);
    let dir = engine.capture_dir().display().to_string();
    Ok(Json(json!({ "recording": body.on, "dir": dir })))
}

#[derive(Deserialize)]
struct ModeBody {
    mode: String,
}

async fn target_mode(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<ModeBody>,
) -> Result<StatusCode, StatusCode> {
    guard(&headers)?;
    if ![
        "bossTargets",
        "mostDamage",
        "lastHitByMe",
        "allTargets",
        "trainTargets",
    ]
    .contains(&body.mode.as_str())
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    engine.set_target_mode(&body.mode);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct FightQuery {
    #[serde(default)]
    query: String,
    #[serde(default)]
    character: String,
    from: Option<i64>,
    to: Option<i64>,
    #[serde(default)]
    favorites: bool,
    #[serde(default)]
    offset: i64,
}
async fn search_fights(
    State(e): State<AppState>,
    Query(q): Query<FightQuery>,
) -> Result<Json<Value>, StatusCode> {
    e.db.search_fights(
        &q.query,
        &q.character,
        q.from.unwrap_or(0),
        q.to.unwrap_or(i64::MAX),
        q.favorites,
        q.offset.max(0),
    )
    .map(Json)
    .map_err(db_error)
}
#[derive(Deserialize)]
struct Annotation {
    favorite: bool,
    note: String,
    tags: String,
}
async fn annotate(
    State(e): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(a): Json<Annotation>,
) -> Result<StatusCode, StatusCode> {
    guard(&headers)?;
    // Characters, like the inputs' maxlength; umlauts and Hangul take several bytes.
    if a.note.chars().count() > 4000 || a.tags.chars().count() > 500 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if e.db
        .annotate(&id, a.favorite, &a.note, &a.tags)
        .map_err(db_error)?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
async fn player(State(e): State<AppState>, Path(id): Path<i32>) -> Json<Value> {
    Json(e.player_details(id))
}
#[derive(Deserialize)]
struct ProfileBody {
    key: String,
    save: bool,
}
async fn profile(
    State(e): State<AppState>,
    headers: HeaderMap,
    Json(p): Json<ProfileBody>,
) -> Result<Json<OverlaySettings>, StatusCode> {
    guard(&headers)?;
    if p.key.trim().is_empty() || p.key.chars().count() > 100 {
        return Err(StatusCode::BAD_REQUEST);
    }
    e.profile(&p.key, p.save)
        .map(Json)
        .map_err(|_| StatusCode::NOT_FOUND)
}
#[derive(Deserialize)]
struct TrainingBody {
    seconds: i64,
}
async fn training(
    State(e): State<AppState>,
    headers: HeaderMap,
    Json(t): Json<TrainingBody>,
) -> Result<StatusCode, StatusCode> {
    guard(&headers)?;
    if t.seconds == 0 {
        e.cancel_training();
    } else if [60, 180, 300].contains(&t.seconds) {
        e.start_training(t.seconds);
    } else {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(StatusCode::NO_CONTENT)
}
async fn last_training(State(e): State<AppState>) -> Result<Json<Value>, StatusCode> {
    Ok(Json(
        e.db.meta("last_training")
            .map_err(db_error)?
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(Value::Null),
    ))
}

// Restrict Host to the listening address or an IP literal for an all-interface
// listener. This also blocks DNS rebinding attacks against the local API.
async fn local_request(State(addr): State<SocketAddr>, request: Request, next: Next) -> Response {
    let allowed = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<axum::http::uri::Authority>().ok())
        .is_some_and(|host| {
            let port = host.port_u16().unwrap_or(80);
            let name = host.host().trim_start_matches('[').trim_end_matches(']');
            let ip = name.parse::<std::net::IpAddr>().ok();
            port == addr.port()
                && (name.eq_ignore_ascii_case("localhost")
                    || ip.is_some_and(|ip| {
                        ip.is_loopback() || addr.ip().is_unspecified() || ip == addr.ip()
                    }))
        });
    if !allowed {
        return StatusCode::FORBIDDEN.into_response();
    }
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        axum::http::HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::X_FRAME_OPTIONS,
        axum::http::HeaderValue::from_static("DENY"),
    );
    response
}

pub fn router(engine: AppState, addr: SocketAddr) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/overlay", get(overlay_page))
        .route(
            "/enhancements.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../web/enhancements.js"),
                )
            }),
        )
        .route(
            "/enhancements.css",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
                    include_str!("../web/enhancements.css"),
                )
            }),
        )
        .route("/api/live", get(live))
        .route("/api/runs", get(runs))
        .route("/api/runs/{id}", get(run_detail).delete(delete_run))
        .route("/api/runs/{id}/note", post(run_note))
        .route("/api/fights", get(search_fights))
        .route("/api/fights/{id}", get(fight_detail))
        .route("/api/fights/{id}/annotation", post(annotate))
        .route("/api/players/{id}", get(player))
        .route("/api/overlay/profile", post(profile))
        .route("/api/training", get(last_training).post(training))
        .route("/api/characters", get(characters))
        .route("/api/stats/partners", get(partners))
        .route("/api/stats/summary", get(summary))
        .route("/api/stats/boss-history", get(boss_history))
        .route("/api/overlay", get(get_overlay).post(set_overlay))
        .route("/api/overlay/toggle-lock", post(toggle_lock))
        .route("/api/overlay/toggle-visible", post(toggle_visible))
        .route("/api/reset", post(reset))
        .route("/api/record", post(set_record))
        .route("/api/record/toggle", post(toggle_record))
        .route("/api/target-mode", post(target_mode))
        .with_state(engine)
        .layer(middleware::from_fn_with_state(addr, local_request))
}

pub async fn serve(engine: AppState, listener: std::net::TcpListener) -> anyhow::Result<()> {
    let addr = listener.local_addr()?;
    let listener = tokio::net::TcpListener::from_std(listener)?;
    tracing::info!("Dashboard: http://{addr}/");
    axum::serve(listener, router(engine, addr)).await?;
    Ok(())
}

/// One request to a running meter (for KDE shortcuts and scripts).
pub fn request(addr: SocketAddr, method: &str, path: &str) -> anyhow::Result<String> {
    use std::io::{Read, Write};
    let timeout = std::time::Duration::from_secs(5);
    let mut stream = std::net::TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\n{ACTION_HEADER}: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    let status = response.lines().next().unwrap_or_default().to_string();
    let body = response
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or_default()
        .to_string();
    if !status.contains(" 2") {
        anyhow::bail!("{status}");
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use tower::ServiceExt;

    fn app() -> Router {
        let engine = Engine::new(
            crate::db::Db::in_memory().unwrap(),
            "de",
            std::env::temp_dir(),
        );
        router(engine, "127.0.0.1:8787".parse().unwrap())
    }

    fn request(method: &str, path: &str, host: &str, action: bool, body: &str) -> Request<Body> {
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, host)
            .header(header::CONTENT_TYPE, "application/json");
        if action {
            builder = builder.header(ACTION_HEADER, "1");
        }
        builder.body(Body::from(body.to_string())).unwrap()
    }

    #[tokio::test]
    async fn local_reads_are_uncached_and_rebinding_hosts_are_rejected() {
        for host in ["localhost:8787", "127.0.0.1:8787", "[::1]:8787"] {
            let response = app()
                .oneshot(request("GET", "/api/live", host, false, ""))
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            assert_eq!(
                response.headers()[header::X_CONTENT_TYPE_OPTIONS],
                "nosniff"
            );
        }
        for host in [
            "evil.example:8787",
            "127.0.0.1:9999",
            "192.168.1.20:8787",
            "localhost.evil.example:8787",
        ] {
            assert_eq!(
                app()
                    .oneshot(request("GET", "/api/live", host, false, ""))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
        }
    }

    #[tokio::test]
    async fn mutations_require_header_and_known_target_modes() {
        assert_eq!(
            app()
                .oneshot(request("POST", "/api/reset", "localhost:8787", false, ""))
                .await
                .unwrap()
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            app()
                .oneshot(request("POST", "/api/reset", "localhost:8787", true, ""))
                .await
                .unwrap()
                .status(),
            StatusCode::NO_CONTENT
        );
        for (mode, status) in [
            ("bossTargets", StatusCode::NO_CONTENT),
            ("garbage", StatusCode::BAD_REQUEST),
        ] {
            let body = json!({"mode": mode}).to_string();
            assert_eq!(
                app()
                    .oneshot(request(
                        "POST",
                        "/api/target-mode",
                        "localhost:8787",
                        true,
                        &body
                    ))
                    .await
                    .unwrap()
                    .status(),
                status
            );
        }
    }

    #[tokio::test]
    async fn overlay_settings_are_clamped_and_survive_the_next_read() {
        let app = app();
        let body = json!({"visible": false, "locked": true, "opacity": 2.0, "scale": 9.0, "max_rows": 100, "show_dps": false, "hide_names": true}).to_string();
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/overlay",
                "localhost:8787",
                true,
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let response = app
            .oneshot(request("GET", "/api/overlay", "localhost:8787", false, ""))
            .await
            .unwrap();
        let data: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
        assert_eq!(data["opacity"], 1.0);
        assert_eq!(data["scale"], 2.5);
        assert_eq!(data["max_rows"], 24);
        assert_eq!(data["hide_names"], true);
        assert_eq!(data["visible"], false);
    }
    #[tokio::test]
    async fn new_actions_require_guard_and_training_duration_is_validated() {
        for (path, body) in [
            ("/api/training", r#"{"seconds":60}"#),
            ("/api/overlay/profile", r#"{"key":"Me","save":true}"#),
            (
                "/api/fights/missing/annotation",
                r#"{"favorite":true,"note":"","tags":""}"#,
            ),
        ] {
            assert_eq!(
                app()
                    .oneshot(request("POST", path, "localhost:8787", false, body))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
        }
        for (seconds, status) in [
            (60, StatusCode::NO_CONTENT),
            (180, StatusCode::NO_CONTENT),
            (300, StatusCode::NO_CONTENT),
            (0, StatusCode::NO_CONTENT),
            (59, StatusCode::BAD_REQUEST),
        ] {
            assert_eq!(
                app()
                    .oneshot(request(
                        "POST",
                        "/api/training",
                        "localhost:8787",
                        true,
                        &json!({"seconds":seconds}).to_string()
                    ))
                    .await
                    .unwrap()
                    .status(),
                status
            );
        }
        assert_eq!(
            app()
                .oneshot(request(
                    "GET",
                    "/enhancements.js",
                    "localhost:8787",
                    false,
                    ""
                ))
                .await
                .unwrap()
                .headers()[header::CONTENT_TYPE],
            "text/javascript; charset=utf-8"
        );
    }
}
