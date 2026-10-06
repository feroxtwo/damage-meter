//! The local web app: live meter, run history and statistics.

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
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
    if headers.contains_key(ACTION_HEADER) { Ok(()) } else { Err(StatusCode::FORBIDDEN) }
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
    (
        [(header::CACHE_CONTROL, "no-store")],
        Json(engine.live()),
    )
        .into_response()
}

#[derive(Deserialize)]
struct RunsQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    dungeon: Option<String>,
}

async fn runs(State(engine): State<AppState>, Query(q): Query<RunsQuery>) -> Result<Json<Value>, StatusCode> {
    let limit = q.limit.unwrap_or(50).clamp(1, 500);
    let offset = q.offset.unwrap_or(0).max(0);
    let dungeon = q.dungeon.as_deref().filter(|d| !d.is_empty());
    engine.db.list_runs(limit, offset, dungeon).map(Json).map_err(db_error)
}

async fn run_detail(State(engine): State<AppState>, Path(id): Path<i64>) -> Result<Json<Value>, StatusCode> {
    engine.db.run_detail(id).map_err(db_error)?.map(Json).ok_or(StatusCode::NOT_FOUND)
}

async fn delete_run(State(engine): State<AppState>, headers: HeaderMap, Path(id): Path<i64>) -> Result<StatusCode, StatusCode> {
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
    engine.db.set_run_note(id, body.note.trim()).map_err(db_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn fight_detail(State(engine): State<AppState>, Path(id): Path<String>) -> Result<Json<Value>, StatusCode> {
    engine.db.fight_detail(&id).map_err(db_error)?.map(Json).ok_or(StatusCode::NOT_FOUND)
}

#[derive(Deserialize)]
struct LimitQuery {
    limit: Option<i64>,
}

async fn partners(State(engine): State<AppState>, Query(q): Query<LimitQuery>) -> Result<Json<Value>, StatusCode> {
    let rows = engine.db.top_partners(q.limit.unwrap_or(5).clamp(1, 100)).map_err(db_error)?;
    Ok(Json(Value::Array(rows)))
}

async fn boss_history(State(engine): State<AppState>) -> Result<Json<Value>, StatusCode> {
    engine.db.boss_history().map(Json).map_err(db_error)
}

async fn summary(State(engine): State<AppState>) -> Result<Json<Value>, StatusCode> {
    engine.db.summary().map(Json).map_err(db_error)
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
    let mut s = engine.overlay.write();
    *s = body;
    s.opacity = s.opacity.clamp(0.0, 1.0);
    s.scale = s.scale.clamp(0.6, 2.5);
    s.max_rows = s.max_rows.clamp(1, 24);
    Ok(Json(s.clone()))
}

async fn toggle_lock(State(engine): State<AppState>, headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
    guard(&headers)?;
    let mut s = engine.overlay.write();
    s.locked = !s.locked;
    Ok(Json(json!({ "locked": s.locked })))
}

async fn toggle_visible(State(engine): State<AppState>, headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
    guard(&headers)?;
    let mut s = engine.overlay.write();
    s.visible = !s.visible;
    Ok(Json(json!({ "visible": s.visible })))
}

async fn reset(State(engine): State<AppState>, headers: HeaderMap) -> Result<StatusCode, StatusCode> {
    guard(&headers)?;
    engine.request_reset();
    Ok(StatusCode::NO_CONTENT)
}

async fn toggle_record(State(engine): State<AppState>, headers: HeaderMap) -> Result<Json<Value>, StatusCode> {
    guard(&headers)?;
    let on = engine.toggle_recording();
    let dir = engine.capture_dir().display().to_string();
    Ok(Json(json!({ "recording": on, "dir": dir })))
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
    engine.set_target_mode(&body.mode);
    Ok(StatusCode::NO_CONTENT)
}

pub fn router(engine: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/overlay", get(overlay_page))
        .route("/api/live", get(live))
        .route("/api/runs", get(runs))
        .route("/api/runs/{id}", get(run_detail).delete(delete_run))
        .route("/api/runs/{id}/note", post(run_note))
        .route("/api/fights/{id}", get(fight_detail))
        .route("/api/stats/partners", get(partners))
        .route("/api/stats/summary", get(summary))
        .route("/api/stats/boss-history", get(boss_history))
        .route("/api/overlay", get(get_overlay).post(set_overlay))
        .route("/api/overlay/toggle-lock", post(toggle_lock))
        .route("/api/overlay/toggle-visible", post(toggle_visible))
        .route("/api/reset", post(reset))
        .route("/api/record/toggle", post(toggle_record))
        .route("/api/target-mode", post(target_mode))
        .with_state(engine)
}

pub async fn serve(engine: AppState, addr: SocketAddr) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Dashboard: http://{addr}/");
    axum::serve(listener, router(engine)).await?;
    Ok(())
}

/// One request to a running meter (for KDE shortcuts and scripts).
pub fn request(addr: SocketAddr, method: &str, path: &str) -> anyhow::Result<String> {
    use std::io::{Read, Write};
    let mut stream = std::net::TcpStream::connect(addr)?;
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\n{ACTION_HEADER}: 1\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    )?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    let status = response.lines().next().unwrap_or_default().to_string();
    let body = response.split("\r\n\r\n").nth(1).unwrap_or_default().to_string();
    if !status.contains(" 2") {
        anyhow::bail!("{status}");
    }
    Ok(body)
}
