//! The local web app: live meter, run history and statistics.

use std::net::SocketAddr;
use std::sync::Arc;

use crate::db::{FightKind, FightSearch};
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

/// Database queries and parser snapshots block; run them off the two async
/// workers so a long history query never delays `/api/live` or the overlay API.
async fn blocking<T: Send + 'static>(
    engine: AppState,
    f: impl FnOnce(&Engine) -> T + Send + 'static,
) -> Result<T, StatusCode> {
    tokio::task::spawn_blocking(move || f(&engine))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

fn db_error(e: anyhow::Error) -> StatusCode {
    tracing::error!("Database: {e:#}");
    StatusCode::INTERNAL_SERVER_ERROR
}

async fn version() -> Json<Value> {
    Json(crate::updates::info())
}
async fn update_check(headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
    guard(&headers)?;
    crate::updates::check().await.map(Json).map_err(|e| {
        tracing::warn!("Release check: {e}");
        StatusCode::BAD_GATEWAY
    })
}
fn display(engine: &Engine, mut value: Value) -> Value {
    crate::skills::decorate(&mut value, &engine.overlay.read().skill_language);
    value
}
async fn skill_icon(Path(name): Path<String>) -> Response {
    match crate::skills::icon(&name) {
        // Bundled into the binary, so they only change with an update. The OBS
        // overlay redraws its rows every 500 ms; without this every class icon
        // was fetched again on each redraw and flickered.
        Some(bytes) => (
            [
                (header::CONTENT_TYPE, "image/webp"),
                (header::CACHE_CONTROL, "public, max-age=86400"),
            ],
            bytes,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}
async fn skill_catalog() -> Json<&'static Value> {
    Json(&crate::skills::ALL)
}
async fn qol_js() -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
        include_str!("../web/qol.js"),
    )
}
async fn index() -> Html<&'static str> {
    Html(include_str!("../web/index.html"))
}

async fn overlay_page() -> Html<&'static str> {
    Html(include_str!("../web/overlay.html"))
}

async fn live(State(engine): State<AppState>) -> Response {
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(display(
            &engine,
            serde_json::to_value(engine.live()).unwrap_or(Value::Null),
        )),
    )
        .into_response()
}

#[derive(Deserialize)]
struct RunsQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    dungeon: Option<String>,
    character: Option<String>,
    #[serde(default)]
    favorites: bool,
}

async fn runs(
    State(engine): State<AppState>,
    Query(q): Query<RunsQuery>,
) -> Result<Json<Value>, StatusCode> {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let offset = q.offset.unwrap_or(0).max(0);
    let dungeon = q.dungeon.filter(|d| !d.is_empty());
    let character = q.character.unwrap_or_default();
    blocking(engine, move |e| {
        e.db.list_runs(limit, offset, dungeon.as_deref(), &character, q.favorites)
    })
    .await?
    .map(Json)
    .map_err(db_error)
}

async fn run_detail(
    State(engine): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, StatusCode> {
    blocking(engine, move |e| {
        e.db.run_detail(id).map(|v| v.map(|v| display(e, v)))
    })
    .await?
    .map_err(db_error)?
    .map(Json)
    .ok_or(StatusCode::NOT_FOUND)
}

async fn run_analysis(
    State(engine): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, StatusCode> {
    blocking(engine, move |e| {
        e.db.run_analysis(id).map(|v| v.map(|v| display(e, v)))
    })
    .await?
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
    if !engine.db.delete_run(id).map_err(db_error)? {
        return Err(StatusCode::CONFLICT);
    }
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
    if body.note.chars().count() > 4000 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let saved = engine
        .db
        .set_run_note(id, body.note.trim())
        .map_err(db_error)?;
    if !saved {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct FavoriteBody {
    favorite: bool,
}

async fn run_favorite(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    Json(body): Json<FavoriteBody>,
) -> Result<StatusCode, StatusCode> {
    guard(&headers)?;
    let saved = blocking(engine, move |e| e.db.set_run_favorite(id, body.favorite))
        .await?
        .map_err(db_error)?;
    if !saved {
        return Err(StatusCode::NOT_FOUND);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn fight_detail(
    State(engine): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    blocking(engine, move |e| {
        e.db.fight_detail(&id).map(|v| v.map(|v| display(e, v)))
    })
    .await?
    .map_err(db_error)?
    .map(Json)
    .ok_or(StatusCode::NOT_FOUND)
}

/// Personal or overall local peer reference for a stored boss fight.
async fn skill_index(
    State(engine): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    blocking(engine, move |e| e.db.skill_index(&id))
        .await?
        .map_err(db_error)?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

/// List reference sources and locally imported snapshots. No third-party requests.
async fn community_sources(State(engine): State<AppState>) -> Result<Json<Value>, StatusCode> {
    blocking(engine, move |e| e.db.community_sources())
        .await?
        .map(Json)
        .map_err(db_error)
}

/// Explicit local import only: the user attests that reuse is permitted.
async fn import_community(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<crate::community::CommunitySnapshot>,
) -> Result<Json<Value>, StatusCode> {
    guard(&headers)?;
    let body = body.validate().map_err(|_| StatusCode::BAD_REQUEST)?;
    blocking(engine, move |e| e.db.import_community(body))
        .await?
        .map(Json)
        .map_err(db_error)
}

/// Explicit deletion of an imported reference period, never of combat data.
async fn delete_community(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Path((source, balance)): Path<(String, String)>,
) -> Result<StatusCode, StatusCode> {
    guard(&headers)?;
    let deleted = blocking(engine, move |e| e.db.delete_community(&source, &balance))
        .await?
        .map_err(db_error)?;
    Ok(if deleted {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    })
}

#[derive(Deserialize)]
struct CommunityRegion {
    #[serde(default = "default_reference_region")]
    region: String,
}

fn default_reference_region() -> String {
    "ALL".to_string()
}

async fn community_index(
    State(engine): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<CommunityRegion>,
) -> Result<Json<Value>, StatusCode> {
    if !crate::community::valid_region(&query.region) {
        return Err(StatusCode::BAD_REQUEST);
    }
    blocking(engine, move |e| e.db.community_index(&id, &query.region))
        .await?
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
    let limit = q.limit.unwrap_or(5).clamp(1, 100);
    let rows = blocking(engine, move |e| e.db.top_partners(limit, &q.character))
        .await?
        .map_err(db_error)?;
    Ok(Json(Value::Array(rows)))
}

async fn boss_history(
    State(engine): State<AppState>,
    Query(q): Query<StatsQuery>,
) -> Result<Json<Value>, StatusCode> {
    blocking(engine, move |e| e.db.boss_history(&q.character))
        .await?
        .map(Json)
        .map_err(db_error)
}

async fn run_history(
    State(engine): State<AppState>,
    Query(q): Query<StatsQuery>,
) -> Result<Json<Value>, StatusCode> {
    blocking(engine, move |e| e.db.run_history(&q.character))
        .await?
        .map(Json)
        .map_err(db_error)
}

async fn summary(
    State(engine): State<AppState>,
    Query(q): Query<StatsQuery>,
) -> Result<Json<Value>, StatusCode> {
    blocking(engine, move |e| e.db.summary(&q.character))
        .await?
        .map(Json)
        .map_err(db_error)
}

async fn characters(State(engine): State<AppState>) -> Result<Json<Value>, StatusCode> {
    blocking(engine, |e| e.db.characters())
        .await?
        .map(|c| Json(Value::Array(c)))
        .map_err(db_error)
}

async fn get_overlay(State(engine): State<AppState>) -> Json<OverlaySettings> {
    Json(engine.overlay.read().clone())
}

async fn set_overlay(
    State(engine): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<OverlaySettings>, StatusCode> {
    guard(&headers)?;
    // A partial update: the dashboard sends only what the user changed, so a
    // stale copy cannot move the overlay back or undo a lock from a shortcut.
    if !body.is_object() {
        return Err(StatusCode::BAD_REQUEST);
    }
    engine.patch_overlay(&body).map(Json).map_err(db_error)
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

async fn new_run(
    State(engine): State<AppState>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    guard(&headers)?;
    engine.request_new_run();
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
    limit: Option<i64>,
    #[serde(default)]
    offset: i64,
    #[serde(default)]
    kind: FightKind,
}
async fn search_fights(
    State(e): State<AppState>,
    Query(q): Query<FightQuery>,
) -> Result<Json<Value>, StatusCode> {
    if q.query.chars().count() > 500 || q.from.zip(q.to).is_some_and(|(a, b)| a > b) {
        return Err(StatusCode::BAD_REQUEST);
    }
    blocking(e, move |e| {
        e.db.search_fights(&FightSearch {
            query: q.query,
            character: q.character,
            from: q.from.unwrap_or(0),
            to: q.to.unwrap_or(i64::MAX),
            favorites: q.favorites,
            kind: q.kind,
            limit: q.limit.unwrap_or(100).clamp(1, 500),
            offset: q.offset.max(0),
        })
    })
    .await?
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
async fn player(State(e): State<AppState>, Path(id): Path<i32>) -> Result<Json<Value>, StatusCode> {
    blocking(e, move |e| display(e, e.player_details(id)))
        .await
        .map(Json)
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
    headers
        .entry(header::CACHE_CONTROL)
        .or_insert(axum::http::HeaderValue::from_static("no-store"));
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
        .route("/qol.js", get(qol_js))
        .route("/assets/icons/{name}", get(skill_icon))
        .route("/api/skills", get(skill_catalog))
        .route(
            "/skills.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../web/skills.js"),
                )
            }),
        )
        .route(
            "/community.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../web/community.js"),
                )
            }),
        )
        .route(
            "/run-analysis.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript; charset=utf-8")],
                    include_str!("../web/run-analysis.js"),
                )
            }),
        )
        .route("/api/version", get(version))
        .route("/api/update-check", post(update_check))
        .route("/api/live", get(live))
        .route("/api/runs", get(runs))
        .route("/api/runs/{id}", get(run_detail).delete(delete_run))
        .route("/api/runs/{id}/analysis", get(run_analysis))
        .route("/api/runs/{id}/note", post(run_note))
        .route("/api/runs/{id}/favorite", post(run_favorite))
        .route("/api/fights", get(search_fights))
        .route("/api/fights/{id}", get(fight_detail))
        .route("/api/fights/{id}/skill-index", get(skill_index))
        .route("/api/fights/{id}/community-index", get(community_index))
        .route("/api/references/sources", get(community_sources))
        .route("/api/references/import", post(import_community))
        .route("/api/references/{source}/{balance}", axum::routing::delete(delete_community))
        .route("/api/fights/{id}/annotation", post(annotate))
        .route("/api/players/{id}", get(player))
        .route("/api/overlay/profile", post(profile))
        .route("/api/training", get(last_training).post(training))
        .route("/api/characters", get(characters))
        .route("/api/stats/partners", get(partners))
        .route("/api/stats/summary", get(summary))
        .route("/api/stats/boss-history", get(boss_history))
        .route("/api/stats/run-history", get(run_history))
        .route("/api/overlay", get(get_overlay).post(set_overlay))
        .route("/api/overlay/toggle-lock", post(toggle_lock))
        .route("/api/overlay/toggle-visible", post(toggle_visible))
        .route("/api/reset", post(reset))
        .route("/api/run/new", post(new_run))
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
    let shutdown = engine.clone();
    axum::serve(listener, router(engine, addr))
        .with_graceful_shutdown(async move {
            while !shutdown.is_stopping() {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        })
        .await?;
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
    async fn community_sources_are_read_only_and_import_requires_action_header() {
        let app = app();
        let list = app
            .clone()
            .oneshot(request(
                "GET",
                "/api/references/sources",
                "localhost:8787",
                false,
                "",
            ))
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);
        let dataset = json!({
            "schema": "a2m-community-v1",
            "source": {
                "id": "community", "url": "https://example.org/statistics",
                "captured_at": 1_790_000_000_000_i64,
                "rights_confirmed": true
            },
            "balance": {
                "id": "period-1", "from_ms": 1_780_000_000_000_i64,
                "until_ms": 1_792_000_000_000_i64
            },
            "rows": [{
                "region": "EU", "dungeon_id": 600093, "mob_code": 2300409,
                "class_key": "gladiator", "cp_min": 60000, "cp_max": 80000,
                "median_dps": 14500.0, "samples": 30
            }]
        })
        .to_string();
        let blocked = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/references/import",
                "localhost:8787",
                false,
                &dataset,
            ))
            .await
            .unwrap();
        assert_eq!(blocked.status(), StatusCode::FORBIDDEN);
        let imported = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/references/import",
                "localhost:8787",
                true,
                &dataset,
            ))
            .await
            .unwrap();
        assert_eq!(imported.status(), StatusCode::OK);
        let removed = app
            .clone()
            .oneshot(request(
                "DELETE",
                "/api/references/community/period-1",
                "localhost:8787",
                true,
                "",
            ))
            .await
            .unwrap();
        assert_eq!(removed.status(), StatusCode::NO_CONTENT);
        let missing = app
            .clone()
            .oneshot(request(
                "GET",
                "/api/fights/missing/community-index?region=EU",
                "localhost:8787",
                false,
                "",
            ))
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        let wrong_region = app
            .oneshot(request(
                "GET",
                "/api/fights/missing/community-index?region=HACK",
                "localhost:8787",
                false,
                "",
            ))
            .await
            .unwrap();
        assert_eq!(wrong_region.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn run_analysis_is_read_only_handles_empty_missing_runs_and_serves_script() {
        let engine = Engine::new(
            crate::db::Db::in_memory().unwrap(),
            "de",
            std::env::temp_dir(),
        );
        let id = engine.db.start_run(600093, 1000, Some("Me"), 1).unwrap();
        let app = router(engine, "127.0.0.1:8787".parse().unwrap());
        let response = app
            .clone()
            .oneshot(request(
                "GET",
                &format!("/api/runs/{id}/analysis"),
                "localhost:8787",
                false,
                "",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let report: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1_000_000).await.unwrap())
                .unwrap();
        assert_eq!(report["id"], id);
        assert_eq!(report["fights"], json!([]));
        let response = app
            .clone()
            .oneshot(request(
                "GET",
                "/api/runs/999999/analysis",
                "localhost:8787",
                false,
                "",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                &format!("/api/runs/{id}/analysis"),
                "localhost:8787",
                true,
                "{}",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        let response = app
            .oneshot(request(
                "GET",
                "/run-analysis.js",
                "localhost:8787",
                false,
                "",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "text/javascript; charset=utf-8"
        );
        let script = to_bytes(response.into_body(), 100_000).await.unwrap();
        assert!(String::from_utf8_lossy(&script).contains("function runCombatModel"));
    }

    #[tokio::test]
    async fn offline_catalog_icons_and_language_persistence() {
        let engine = Engine::new(
            crate::db::Db::in_memory().unwrap(),
            "en",
            std::env::temp_dir(),
        );
        let app = router(engine.clone(), "127.0.0.1:8787".parse().unwrap());
        let response = app
            .clone()
            .oneshot(request("GET", "/api/skills", "localhost:8787", false, ""))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let catalog: Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 4_000_000).await.unwrap())
                .unwrap();
        assert_eq!(catalog["skills"].as_array().unwrap().len(), 364);
        assert_eq!(
            catalog["skills"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|s| s["icon"].is_string())
                .count(),
            353
        );
        let response = app
            .clone()
            .oneshot(request(
                "GET",
                "/assets/icons/skill-11170000.webp",
                "localhost:8787",
                false,
                "",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "image/webp");
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "public, max-age=86400"
        );
        let bytes = to_bytes(response.into_body(), 100_000).await.unwrap();
        assert!(bytes.starts_with(b"RIFF"));
        assert_eq!(&bytes[8..12], b"WEBP");
        let response = app
            .clone()
            .oneshot(request(
                "GET",
                "/assets/icons/not-bundled.webp",
                "localhost:8787",
                false,
                "",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(engine.overlay.read().skill_language, "en");
        engine
            .db
            .set_meta("overlay_profile:legacy", r#"{"theme":"ember"}"#)
            .unwrap();
        assert_eq!(
            engine.profile("legacy", false).unwrap().skill_language,
            "en"
        );
        engine.profile("language", true).unwrap();
        let response = app
            .clone()
            .oneshot(request(
                "POST",
                "/api/overlay",
                "localhost:8787",
                true,
                r#"{"skill_language":"de"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let saved: Value =
            serde_json::from_str(&engine.db.meta("overlay").unwrap().unwrap()).unwrap();
        assert_eq!(saved["skill_language"], "de");
        assert_eq!(
            engine.profile("language", false).unwrap().skill_language,
            "en"
        );
        let response = app
            .oneshot(request(
                "POST",
                "/api/overlay",
                "localhost:8787",
                true,
                r#"{"skill_language":"xx"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(engine.overlay.read().skill_language, "de");
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
    async fn searches_notes_and_active_run_deletion_validate_input() {
        let engine = Engine::new(
            crate::db::Db::in_memory().unwrap(),
            "de",
            std::env::temp_dir(),
        );
        let id = engine.db.start_run(600093, 1000, Some("Me"), 1).unwrap();
        let app = router(engine.clone(), "127.0.0.1:8787".parse().unwrap());
        for (method, path, body, status) in [
            (
                "GET",
                "/api/fights?from=2000&to=1000".to_string(),
                "".to_string(),
                StatusCode::BAD_REQUEST,
            ),
            (
                "GET",
                format!("/api/fights?query={}", "a".repeat(501)),
                "".into(),
                StatusCode::BAD_REQUEST,
            ),
            (
                "POST",
                "/api/runs/999/favorite".into(),
                json!({"favorite":true}).to_string(),
                StatusCode::NOT_FOUND,
            ),
            (
                "POST",
                "/api/runs/999/note".into(),
                json!({"note":"missing"}).to_string(),
                StatusCode::NOT_FOUND,
            ),
            (
                "POST",
                format!("/api/runs/{id}/note"),
                json!({"note":"ü".repeat(4001)}).to_string(),
                StatusCode::BAD_REQUEST,
            ),
            (
                "DELETE",
                format!("/api/runs/{id}"),
                "".into(),
                StatusCode::CONFLICT,
            ),
        ] {
            let response = app
                .clone()
                .oneshot(request(method, &path, "localhost:8787", true, &body))
                .await
                .unwrap();
            assert_eq!(response.status(), status);
        }
        engine
            .db
            .upsert_members(
                id,
                &[crate::db::Member {
                    name: "Other".into(),
                    job: "검성".into(),
                    server_id: 1,
                    level: 45,
                    gear_score: 0,
                    combat_power: 0,
                    dbid: 0,
                    is_self: false,
                }],
            )
            .unwrap();
        engine.db.end_run(id, 2000).unwrap();
        assert_eq!(
            app.oneshot(request(
                "DELETE",
                &format!("/api/runs/{id}"),
                "localhost:8787",
                true,
                ""
            ))
            .await
            .unwrap()
            .status(),
            StatusCode::NO_CONTENT
        );
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
    async fn partial_overlay_updates_keep_position_and_lock_from_other_sources() {
        let engine = Engine::new(
            crate::db::Db::in_memory().unwrap(),
            "de",
            std::env::temp_dir(),
        );
        let app = router(engine.clone(), "127.0.0.1:8787".parse().unwrap());
        // The overlay was dragged and locked by a shortcut after the dashboard loaded.
        engine
            .modify_overlay(|s| {
                s.position = Some([700.0, 300.0]);
                s.locked = true;
            })
            .unwrap();
        let post = |body: Value| {
            app.clone().oneshot(request(
                "POST",
                "/api/overlay",
                "localhost:8787",
                true,
                &body.to_string(),
            ))
        };
        let response = post(json!({"opacity": 0.4, "theme": "ember"}))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let s = engine.overlay.read().clone();
        assert_eq!(s.position, Some([700.0, 300.0]));
        assert!(s.locked);
        assert_eq!(s.theme, "ember");
        assert!((s.opacity - 0.4).abs() < 1e-6);
        // A wrong type in one field leaves that field and all others unchanged.
        post(json!({"scale": "big", "max_rows": 3})).await.unwrap();
        let s = engine.overlay.read().clone();
        assert_eq!(s.scale, 1.0);
        assert_eq!(s.max_rows, 3);
        assert_eq!(
            post(json!([1, 2])).await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
    }
    #[tokio::test]
    async fn unicode_text_limits_accept_multibyte_characters_and_reject_overlong_inputs() {
        let app = app();
        for (count, status) in [(100, StatusCode::OK), (101, StatusCode::BAD_REQUEST)] {
            let body = json!({"key":"한".repeat(count),"save":true}).to_string();
            let response = app
                .clone()
                .oneshot(request(
                    "POST",
                    "/api/overlay/profile",
                    "localhost:8787",
                    true,
                    &body,
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), status);
        }
        for (note_len, tags_len, status) in [
            // Missing fight gives 404 only after the text has passed validation.
            (4000, 500, StatusCode::NOT_FOUND),
            (4001, 500, StatusCode::BAD_REQUEST),
            (4000, 501, StatusCode::BAD_REQUEST),
        ] {
            let body =
                json!({"favorite":false,"note":"ü".repeat(note_len),"tags":"한".repeat(tags_len)})
                    .to_string();
            let response = app
                .clone()
                .oneshot(request(
                    "POST",
                    "/api/fights/missing/annotation",
                    "localhost:8787",
                    true,
                    &body,
                ))
                .await
                .unwrap();
            assert_eq!(response.status(), status);
        }
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
