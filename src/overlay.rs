//! The in-game overlay: a small borderless, transparent, always-on-top window.
//!
//! On KDE Plasma (Wayland) KWin decides what stays on top, so the install
//! script adds a window rule for the app id `aion2-meter` that keeps it above
//! the game (see `scripts/install-kwin-rule.sh`). Under XWayland
//! (`--x11`) the always-on-top request works without a rule.

use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Vec2, ViewportCommand,
};

use crate::engine::{Engine, Live, metric_value, overlay_rows};
use crate::names::{duration, short_number};

/// winit loads this X11 keyboard library dynamically, so package dependency
/// scanners cannot discover it from the executable's ELF imports.
pub fn check_x11_dependencies() -> anyhow::Result<()> {
    let handle = unsafe { libc::dlopen(c"libxkbcommon-x11.so.0".as_ptr(), libc::RTLD_LAZY) };
    anyhow::ensure!(
        !handle.is_null(),
        "X11-Bibliothek libxkbcommon-x11.so.0 fehlt. Debian/Ubuntu: sudo apt install libxkbcommon-x11-0; Fedora: sudo dnf install libxkbcommon-x11. Danach das Meter neu starten."
    );
    unsafe { libc::dlclose(handle) };
    Ok(())
}

pub const APP_ID: &str = "aion2-meter";
const WIDTH: f32 = 360.0;
const HEADER: f32 = 40.0;
const ROW: f32 = 40.0;
fn row_height(compact: bool) -> f32 {
    if compact { 28.0 } else { ROW }
}
fn overlay_width(compact: bool) -> f32 {
    if compact { 312.0 } else { WIDTH }
}
fn header_height(compact: bool) -> f32 {
    if compact { 36.0 } else { HEADER }
}
fn footer_height(compact: bool) -> f32 {
    if compact { 18.0 } else { FOOTER }
}
/// The footer is a setup and alarm strip: always there while unlocked, and in
/// the locked combat mode only when capture, permission, numbers or recording
/// need attention. Everyday status (dungeon, ping) costs no pixels in combat.
fn footer_visible(live: &Live) -> bool {
    let c = &live.capture;
    !live.overlay.locked
        || c.error.is_some()
        || !c.permission
        || c.locked_port.is_none()
        || live.numeric_limited
        || c.recording.is_some()
}
fn overlay_size(compact: bool, rows: usize, footer: bool) -> Vec2 {
    Vec2::new(
        overlay_width(compact),
        header_height(compact)
            + if footer { footer_height(compact) } else { 0.0 }
            + row_height(compact) * rows.max(1) as f32
            + 4.0,
    )
}
/// One design system, three characters: the same layout and hierarchy, with
/// each theme's own ground, accent and corner shape. Gold always marks you.
struct Theme {
    ground: [u8; 3],
    head: [u8; 3],
    accent: [u8; 3],
    me: [u8; 3],
    radius: u8,
}
fn theme(name: &str) -> Theme {
    match name {
        "aether" => Theme {
            ground: [8, 26, 32],
            head: [14, 44, 50],
            accent: [117, 224, 206],
            me: [240, 210, 140],
            radius: 12,
        },
        "ember" => Theme {
            ground: [25, 17, 21],
            head: [46, 28, 30],
            accent: [246, 160, 96],
            me: [255, 214, 140],
            radius: 3,
        },
        _ => Theme {
            ground: [11, 16, 24],
            head: [16, 28, 44],
            accent: [91, 213, 222],
            me: [240, 200, 120],
            radius: 8,
        },
    }
}
const FOOTER: f32 = 22.0;
const ICON: f32 = 18.0;

pub struct Overlay {
    engine: Arc<Engine>,
    web_url: String,
    passthrough: Option<bool>,
    last_size: Vec2,
    last_position: Option<[f32; 2]>,
    last_scale: f32,
    class_icons: std::collections::HashMap<String, egui::TextureHandle>,
    #[cfg(debug_assertions)]
    fixture: Option<Live>,
}

fn masked(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    match chars.len() {
        0..=2 => "*".repeat(chars.len()),
        n => format!("{}{}{}", chars[0], "*".repeat((n - 2).min(4)), chars[n - 1]),
    }
}

fn with_alpha(c: [u8; 3], a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], a)
}

/// A dark contour around each glyph keeps text readable on white skies,
/// bright spell effects and dark caves alike, without an opaque plate behind it.
fn contour(p: &egui::Painter, pos: Pos2, galley: &Arc<egui::Galley>) {
    for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (1.0, 1.5)] {
        p.galley_with_override_text_color(
            pos + Vec2::new(dx, dy),
            galley.clone(),
            Color32::from_black_alpha(if dy > 1.0 { 220 } else { 150 }),
        );
    }
}

fn shadow_text(
    p: &egui::Painter,
    pos: Pos2,
    align: Align2,
    text: impl ToString,
    font: FontId,
    color: Color32,
) -> Rect {
    let galley = p.layout_no_wrap(text.to_string(), font, color);
    let rect = align.anchor_size(pos, galley.size());
    contour(p, rect.min, &galley);
    p.galley(rect.min, galley, color);
    rect
}

/// Single-line text that ends in "…" instead of being cut mid-glyph.
fn shadow_text_elided(
    p: &egui::Painter,
    left_center: Pos2,
    text: String,
    font: FontId,
    color: Color32,
    max_width: f32,
) {
    let mut job = egui::text::LayoutJob::simple_singleline(text, font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width.max(0.0));
    let galley = p.layout_job(job);
    let pos = left_center - Vec2::new(0.0, galley.size().y / 2.0);
    contour(p, pos, &galley);
    p.galley(pos, galley, color);
}

/// Vertical fade: `color` at the bottom edge of `rect`, transparent at the top.
fn fade_up(p: &egui::Painter, rect: Rect, color: Color32) {
    let mut mesh = egui::Mesh::default();
    let clear = Color32::TRANSPARENT;
    mesh.colored_vertex(rect.left_top(), clear);
    mesh.colored_vertex(rect.right_top(), clear);
    mesh.colored_vertex(rect.right_bottom(), color);
    mesh.colored_vertex(rect.left_bottom(), color);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(egui::Shape::mesh(mesh));
}

/// Horizontal fade: `color` at the left edge of `rect`, transparent at the right.
fn fade_right(p: &egui::Painter, rect: Rect, color: Color32) {
    let mut mesh = egui::Mesh::default();
    let clear = Color32::TRANSPARENT;
    mesh.colored_vertex(rect.left_top(), color);
    mesh.colored_vertex(rect.right_top(), clear);
    mesh.colored_vertex(rect.right_bottom(), clear);
    mesh.colored_vertex(rect.left_bottom(), color);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(egui::Shape::mesh(mesh));
}

// Debug-only visual QA input; no packet/API injection and absent from release builds.
#[cfg(debug_assertions)]
fn native_fixture() -> Option<Live> {
    let path = std::env::var_os("A2M_NATIVE_FIXTURE")?;
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() > 1024 * 1024 {
        return None;
    }
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let mut live = Live {
        target_name: "SYNTHETISCH · Kargos".into(),
        target_hp: Some(0.38),
        dungeon: "Synthetischer Hornbau".into(),
        battle_time_ms: 90_000,
        ping_ms: Some(42),
        ..Default::default()
    };
    live.capture.permission = true;
    live.capture.game_running = true;
    live.capture.locked_port = Some(13328);
    for (i, r) in v.get("rows")?.as_array()?.iter().take(24).enumerate() {
        let class = crate::names::class_info(r.get("job")?.as_str()?);
        let damage = r.get("damage")?.as_f64()?.clamp(0.0, 1e15);
        live.rows.push(crate::engine::LiveRow {
            id: i as i32 + 1,
            name: r.get("name")?.as_str()?.chars().take(100).collect(),
            class_key: class.key,
            class_name: class.name,
            color: class.color,
            damage,
            dps: damage / 90.0,
            heal: damage / 5.0,
            hps: damage / 450.0,
            is_self: r.get("self").and_then(|v| v.as_bool()).unwrap_or(i == 0),
            dead: r.get("dead").and_then(|v| v.as_bool()).unwrap_or(i == 3),
            ..Default::default()
        });
    }
    Some(live)
}

impl Overlay {
    pub fn new(engine: Arc<Engine>, web_url: String) -> Self {
        let last_position = engine.overlay.read().position;
        Self {
            last_position,
            engine,
            web_url,
            passthrough: None,
            last_size: Vec2::ZERO,
            last_scale: 0.0,
            class_icons: Default::default(),
            #[cfg(debug_assertions)]
            fixture: native_fixture(),
        }
    }

    pub fn run(mut self) -> eframe::Result {
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("AION2 Meter")
                .with_app_id(APP_ID)
                .with_position(self.engine.overlay.read().position.unwrap_or([40.0, 40.0]))
                .with_inner_size([WIDTH, HEADER + FOOTER + ROW * 2.0])
                .with_min_inner_size([160.0, 48.0])
                .with_decorations(false)
                .with_transparent(true)
                .with_always_on_top()
                .with_taskbar(false)
                .with_resizable(false),
            ..Default::default()
        };
        eframe::run_native(
            "AION2 Meter",
            options,
            Box::new(|cc| {
                for key in crate::skills::CLASS_KEYS {
                    if let Some(rgba) = crate::skills::class_rgba(key) {
                        self.class_icons.insert(
                            key.into(),
                            cc.egui_ctx.load_texture(
                                key,
                                egui::ColorImage::from_rgba_unmultiplied([32, 32], rgba),
                                egui::TextureOptions::LINEAR,
                            ),
                        );
                    }
                }
                Ok(Box::new(self))
            }),
        )
    }

    fn header(&self, ui: &mut egui::Ui, live: &Live, width: f32, bg: u8) {
        let settings = self.engine.overlay.read().clone();
        let t = theme(&settings.theme);
        let compact = settings.compact;
        let (rect, resp) = ui.allocate_exact_size(
            Vec2::new(width, header_height(compact)),
            Sense::click_and_drag(),
        );
        let p = ui.painter();
        let r = t.radius;
        p.rect_filled(
            rect,
            CornerRadius {
                nw: r,
                ne: r,
                sw: 0,
                se: 0,
            },
            with_alpha(t.head, bg.saturating_add(30)),
        );
        // Theme mark: a short accent rule.
        p.line_segment(
            [
                rect.left_top() + Vec2::new(r as f32 + 2.0, 1.0),
                rect.left_top() + Vec2::new(r as f32 + 40.0, 1.0),
            ],
            Stroke::new(2.0_f32, with_alpha(t.accent, 255)),
        );
        let title = if live.target_name.is_empty() {
            "Kein Ziel"
        } else {
            &live.target_name
        };
        let time = duration(live.battle_time_ms);
        let time_rect = shadow_text(
            p,
            rect.right_top() + Vec2::new(-8.0, 4.0),
            Align2::RIGHT_TOP,
            time,
            FontId::monospace(if compact { 13.0 } else { 14.0 }),
            Color32::WHITE,
        );
        if live.capture.recording.is_some() {
            p.circle_filled(
                Pos2::new(time_rect.left() - 8.0, time_rect.center().y),
                3.5,
                Color32::from_rgb(235, 70, 70),
            );
        }
        let mut title_rect = rect;
        title_rect.set_right(time_rect.left() - 16.0);
        shadow_text_elided(
            &p.with_clip_rect(title_rect),
            rect.left_top() + Vec2::new(9.0, if compact { 11.0 } else { 12.0 }),
            title.to_string(),
            FontId::proportional(if compact { 13.0 } else { 14.0 }),
            Color32::WHITE,
            title_rect.width() - 9.0,
        );
        let total = live
            .rows
            .iter()
            .map(|r| metric_value(r, &settings.metric))
            .sum::<f64>();
        let rate = total * 1000.0 / live.battle_time_ms.max(1000) as f64;
        let label = match settings.metric.as_str() {
            "heal" => "HPS",
            "damage_received" => "Erlitten/s",
            _ => "DPS",
        };
        let line2 = rect.top() + if compact { 19.0 } else { 22.0 };
        let group = shadow_text(
            p,
            Pos2::new(rect.left() + 9.0, line2),
            Align2::LEFT_TOP,
            "GRUPPE",
            FontId::proportional(8.5),
            with_alpha(t.accent, 230),
        );
        shadow_text(
            p,
            Pos2::new(group.right() + 5.0, line2 - 1.0),
            Align2::LEFT_TOP,
            format!("{} {label}", short_number(rate)),
            FontId::proportional(10.5),
            Color32::from_gray(225),
        );
        // HP: the readout sits on its own bar, which spans the header's lower edge.
        let bar = Rect::from_min_size(
            rect.left_bottom() + Vec2::new(0.0, -3.0),
            Vec2::new(width, 3.0),
        );
        p.rect_filled(bar, 0, Color32::from_black_alpha(150));
        if let Some(hp) = live.target_hp {
            let mut fill = bar;
            fill.set_width(bar.width() * hp.clamp(0.0, 1.0) as f32);
            p.rect_filled(fill, 0, Color32::from_rgb(238, 92, 106));
            shadow_text(
                p,
                Pos2::new(rect.right() - 8.0, line2 - 1.0),
                Align2::RIGHT_TOP,
                format!(
                    "{}{} % HP",
                    if live.hp_estimated { "~" } else { "" },
                    format!("{:.1}", hp * 100.0).replace('.', ",")
                ),
                FontId::proportional(10.5),
                Color32::from_gray(235),
            );
        }

        // Ask for the move on the press itself, while that press is the
        // newest input event: a Wayland move request names the press it
        // belongs to, and one sent after egui's drag threshold went unheeded.
        let pressed = ui.input(|i| i.pointer.primary_pressed());
        if pressed && resp.hovered() && !self.engine.overlay.read().locked {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
        resp.context_menu(|ui| self.menu(ui));
    }

    fn menu(&self, ui: &mut egui::Ui) {
        if ui.button("Meter zurücksetzen").clicked() {
            self.engine.request_reset();
            ui.close();
        }
        if ui.button("Sperren (Klicks gehen ans Spiel)").clicked() {
            let _ = self.engine.modify_overlay(|s| s.locked = true);
            ui.close();
        }
        let mut recording = self.engine.recording_wanted();
        if ui
            .checkbox(&mut recording, "Pakete mitschneiden")
            .on_hover_text("Bleibt nach einem Neustart an, bis du den Haken entfernst")
            .changed()
        {
            self.engine.set_recording(recording);
        }
        ui.separator();
        let mut compact = self.engine.overlay.read().compact;
        if ui.checkbox(&mut compact, "Kompakte Zeilen").changed() {
            let _ = self.engine.modify_overlay(|s| s.compact = compact);
        }
        for (key, label) in [
            ("midnight", "Midnight"),
            ("aether", "Aether"),
            ("ember", "Ember"),
        ] {
            if ui.button(label).clicked() {
                let _ = self.engine.modify_overlay(|s| s.theme = key.into());
            }
        }
        ui.separator();
        ui.label("Ziel");
        for (id, label) in [
            ("bossTargets", "Bosse"),
            ("mostDamage", "Höchster Schaden"),
            ("lastHitByMe", "Zuletzt von mir getroffen"),
            ("allTargets", "Alle Ziele"),
            ("trainTargets", "Trainingspuppe"),
        ] {
            if ui.button(label).clicked() {
                self.engine.set_target_mode(id);
                ui.close();
            }
        }
        ui.separator();
        if ui.button("Dashboard öffnen").clicked() {
            let _ = std::process::Command::new("xdg-open")
                .arg(&self.web_url)
                .spawn();
            ui.close();
        }
        if ui.button("Beenden").clicked() {
            ui.ctx().send_viewport_cmd(ViewportCommand::Close);
        }
    }

    fn rows(
        &self,
        ui: &mut egui::Ui,
        live: &Live,
        width: f32,
        _max_rows: usize,
        show_dps: bool,
        hide_names: bool,
    ) {
        let settings = self.engine.overlay.read().clone();
        let rows = overlay_rows(live, &settings);
        let total = live
            .rows
            .iter()
            .map(|r| metric_value(r, &settings.metric))
            .sum::<f64>()
            .max(1.0);
        let top = rows
            .iter()
            .map(|(_, r)| metric_value(r, &settings.metric))
            .fold(1.0, f64::max);
        for (i, row) in &rows {
            let value = metric_value(row, &settings.metric);
            let rate = match settings.metric.as_str() {
                "heal" => row.hps,
                "damage_received" => value * 1000.0 / live.battle_time_ms.max(1000) as f64,
                _ => row.dps,
            };
            let (rect, response) = ui.allocate_exact_size(
                Vec2::new(width, row_height(settings.compact)),
                Sense::click(),
            );
            if response.clicked() && !settings.locked {
                let _ = std::process::Command::new("xdg-open")
                    .arg(format!("{}?player={}#live", self.web_url, row.id))
                    .spawn();
            }
            let p = ui.painter();
            let t = theme(&settings.theme);
            let compact = settings.compact;
            let inner = rect.shrink2(Vec2::new(4.0, 1.0));
            let me = with_alpha(t.me, 255);
            let text_color = if row.dead {
                Color32::from_gray(150)
            } else {
                Color32::WHITE
            };
            // Your lane: a gold wash that fades out, plus a gold edge. No box.
            if row.is_self {
                fade_right(p, inner, with_alpha(t.me, 52));
                p.line_segment(
                    [
                        inner.left_top() + Vec2::new(0.0, 2.0),
                        inner.left_bottom() - Vec2::new(0.0, 2.0),
                    ],
                    Stroke::new(2.0_f32, me),
                );
            }
            let name = if hide_names && !row.is_self {
                masked(&row.name)
            } else {
                row.name.clone()
            };
            response.on_hover_text(format!(
                "{}. {}\n{} gesamt · {}/s · {:.1}%{}",
                i + 1,
                name,
                short_number(value),
                short_number(rate),
                value * 100.0 / total,
                if row.dead { "\nTod erfasst" } else { "" },
            ));
            // Rank column: your rank is a filled gold chip, the others plain digits.
            let mid = inner.center().y - if compact { 1.5 } else { 2.0 };
            let rank_center = Pos2::new(inner.left() + 15.0, mid);
            let rank = format!("{:02}", i + 1);
            if row.is_self {
                p.rect_filled(
                    Rect::from_center_size(rank_center, Vec2::new(22.0, 16.0)),
                    3,
                    me,
                );
                p.text(
                    rank_center,
                    Align2::CENTER_CENTER,
                    rank,
                    FontId::monospace(11.0),
                    with_alpha(t.ground, 255),
                );
            } else {
                shadow_text(
                    p,
                    rank_center,
                    Align2::CENTER_CENTER,
                    rank,
                    FontId::monospace(11.0),
                    Color32::from_gray(if row.dead { 140 } else { 215 }),
                );
            }
            let icon = self.class_icons.get(row.class_key);
            if let Some(texture) = icon {
                let icon_rect = Rect::from_center_size(
                    Pos2::new(inner.left() + 38.0, mid),
                    Vec2::splat(if compact { 17.0 } else { 20.0 }),
                );
                // A small dark disc keeps the light class artwork visible on bright scenes.
                p.circle_filled(
                    icon_rect.center(),
                    icon_rect.width() / 2.0 + 1.5,
                    Color32::from_black_alpha(130),
                );
                p.image(
                    texture.id(),
                    icon_rect,
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    text_color,
                );
            }
            // Rate dominates: large figure, small unit. The share follows quietly in normal mode.
            let (figure, unit) = if show_dps {
                (short_number(rate), "/s")
            } else {
                (short_number(value), "")
            };
            let unit_font = FontId::proportional(9.0);
            let unit_rect = shadow_text(
                p,
                Pos2::new(
                    inner.right() - 6.0,
                    if compact {
                        mid + 2.0
                    } else {
                        inner.top() + 15.0
                    },
                ),
                Align2::RIGHT_CENTER,
                unit,
                unit_font,
                Color32::from_gray(185),
            );
            let figure_rect = shadow_text(
                p,
                Pos2::new(
                    unit_rect.left() - 1.0,
                    if compact { mid } else { inner.top() + 13.0 },
                ),
                Align2::RIGHT_CENTER,
                figure,
                FontId::monospace(if compact { 15.0 } else { 17.0 }),
                text_color,
            );
            let mut numbers_left = figure_rect.left();
            if !compact {
                let share = format!("{} · {:.0} %", short_number(value), value * 100.0 / total);
                let r = shadow_text(
                    p,
                    Pos2::new(inner.right() - 6.0, inner.bottom() - 9.0),
                    Align2::RIGHT_CENTER,
                    share,
                    FontId::proportional(9.5),
                    Color32::from_gray(190),
                );
                numbers_left = numbers_left.min(r.left());
            }
            let name_x = inner.left() + if icon.is_some() { 51.0 } else { 32.0 };
            shadow_text_elided(
                p,
                Pos2::new(name_x, mid),
                format!("{}{}", if row.dead { "† " } else { "" }, name),
                FontId::proportional(if compact { 12.0 } else { 13.0 }),
                text_color,
                numbers_left - 10.0 - name_x,
            );
            // Energy track: class colour with a short glow above the fill.
            let track = Rect::from_min_max(
                Pos2::new(name_x, inner.bottom() - 3.0),
                Pos2::new(inner.right() - 6.0, inner.bottom() - 0.5),
            );
            p.rect_filled(track, 1, Color32::from_black_alpha(110));
            let mut fill = track;
            fill.set_width(track.width() * (value / top).clamp(0.0, 1.0) as f32);
            let alpha = if row.dead { 110 } else { 255 };
            fade_up(
                p,
                Rect::from_min_max(
                    fill.left_top() - Vec2::new(0.0, if compact { 6.0 } else { 9.0 }),
                    fill.right_top(),
                ),
                with_alpha(row.color, if row.dead { 20 } else { 70 }),
            );
            p.rect_filled(fill, 1, with_alpha(row.color, alpha));
        }
    }

    fn footer(&self, ui: &mut egui::Ui, live: &Live, width: f32, bg: u8) {
        let (rect, resp) = ui.allocate_exact_size(
            Vec2::new(width, footer_height(live.overlay.compact)),
            Sense::click(),
        );
        let p = ui.painter();
        let t = theme(&live.overlay.theme);
        p.rect_filled(
            rect,
            CornerRadius {
                nw: 0,
                ne: 0,
                sw: t.radius,
                se: t.radius,
            },
            with_alpha(t.head, bg.saturating_add(40)),
        );
        let c = &live.capture;
        let (dot, state) = if c.error.is_some() {
            (
                Color32::from_rgb(255, 125, 137),
                "Capture-Fehler".to_string(),
            )
        } else if !c.permission {
            (
                Color32::from_rgb(220, 60, 60),
                "keine Capture-Berechtigung".to_string(),
            )
        } else if live.numeric_limited {
            (
                Color32::from_rgb(230, 180, 60),
                "Zahlengrenze: Näherungswerte".to_string(),
            )
        } else if c.locked_port.is_some() {
            (
                Color32::from_rgb(80, 200, 100),
                if live.dungeon.is_empty() {
                    "Verbunden".to_string()
                } else {
                    live.dungeon.clone()
                },
            )
        } else if c.game_running {
            (
                Color32::from_rgb(230, 180, 60),
                "suche Verbindung …".to_string(),
            )
        } else {
            (Color32::from_gray(140), "AION2 nicht gestartet".to_string())
        };
        let state = if c.recording.is_some() {
            format!("REC · {state}")
        } else {
            state
        };
        p.circle_filled(rect.left_center() + Vec2::new(10.0, 0.0), 3.5, dot);
        let controls = !live.overlay.locked && ui.rect_contains_pointer(rect);
        let mut state_rect = rect;
        state_rect.set_right(
            rect.right()
                - if controls {
                    4.0 * (ICON + 3.0) + 52.0
                } else {
                    70.0
                },
        );
        p.with_clip_rect(state_rect).text(
            rect.left_center() + Vec2::new(18.0, 0.0),
            Align2::LEFT_CENTER,
            state.clone(),
            FontId::proportional(10.5),
            Color32::from_gray(200),
        );
        // Buttons remain available on hover; combat has no permanent icon rail.
        let mut x = rect.right() - 4.0;
        if controls {
            let mut button = |icon: Icon, tip: &str| {
                let r = Rect::from_center_size(
                    Pos2::new(x - ICON / 2.0, rect.center().y),
                    Vec2::splat(ICON),
                );
                x -= ICON + 3.0;
                let resp = ui
                    .interact(r, ui.id().with(tip), Sense::click())
                    .on_hover_text(tip);
                let color = if resp.hovered() {
                    ui.painter()
                        .rect_filled(r, 4, Color32::from_white_alpha(28));
                    Color32::WHITE
                } else {
                    Color32::from_gray(185)
                };
                paint_icon(ui.painter(), r.shrink(3.5), icon, color);
                resp.clicked()
            };
            let recording = self.engine.recording_wanted();
            if button(
                Icon::Lock,
                "Sperren: Klicks gehen ans Spiel (Strg+Umschalt+F9 entsperrt)",
            ) {
                let _ = self.engine.modify_overlay(|s| s.locked = true);
            }
            if button(
                Icon::Hide,
                "Ausblenden (Strg+Umschalt+F10 blendet wieder ein)",
            ) {
                let _ = self.engine.modify_overlay(|s| s.visible = false);
            }
            if button(Icon::Reset, "Meter zurücksetzen (Strg+Umschalt+F11)") {
                self.engine.request_reset();
            }
            if button(
                Icon::Record(recording),
                if recording {
                    "Mitschnitt läuft: klicken zum Ausschalten"
                } else {
                    "Pakete mitschneiden"
                },
            ) {
                self.engine.set_recording(!recording);
            }
        }
        let ping = live.ping_ms.map(|p| format!("{p} ms")).unwrap_or_default();
        let p = ui.painter();
        p.text(
            Pos2::new(
                if controls {
                    x - 2.0
                } else {
                    rect.right() - 8.0
                },
                rect.center().y,
            ),
            Align2::RIGHT_CENTER,
            ping,
            FontId::monospace(10.5),
            Color32::from_gray(200),
        );
        resp.on_hover_text(state).context_menu(|ui| self.menu(ui));
    }
}

#[derive(Clone, Copy)]
enum Icon {
    Lock,
    Hide,
    Reset,
    /// Whether recording is on.
    Record(bool),
}

/// Points on a circle from angle `a` to `b` (radians, clockwise on screen).
fn arc(center: Pos2, radius: f32, a: f32, b: f32) -> Vec<Pos2> {
    (0..=16)
        .map(|i| {
            let t = a + (b - a) * i as f32 / 16.0;
            center + Vec2::angled(t) * radius
        })
        .collect()
}

/// Small line icons, drawn so they need no icon font.
fn paint_icon(p: &egui::Painter, r: Rect, icon: Icon, color: Color32) {
    use std::f32::consts::PI;
    let stroke = Stroke::new(1.5_f32, color);
    match icon {
        Icon::Lock => {
            let body = Rect::from_min_max(
                Pos2::new(r.left() + 1.0, r.center().y - 0.5),
                r.right_bottom() - Vec2::new(1.0, 0.0),
            );
            p.rect_filled(body, 1.5, color);
            let shackle = r.width() * 0.28;
            let top = Pos2::new(r.center().x, r.top() + shackle + 0.5);
            let mut pts = vec![Pos2::new(top.x - shackle, body.top())];
            pts.extend(arc(top, shackle, PI, 2.0 * PI));
            pts.push(Pos2::new(top.x + shackle, body.top()));
            p.add(egui::Shape::line(pts, stroke));
        }
        Icon::Hide => {
            let c = r.center();
            let (w, h) = (r.width() / 2.0, r.height() * 0.32);
            let eye: Vec<Pos2> = (0..=24)
                .map(|i| {
                    let t = 2.0 * PI * i as f32 / 24.0;
                    Pos2::new(c.x + w * t.cos(), c.y + h * t.sin())
                })
                .collect();
            p.add(egui::Shape::line(eye, stroke));
            p.circle_filled(c, h * 0.6, color);
            p.line_segment([r.left_bottom(), r.right_top()], stroke);
        }
        Icon::Reset => {
            let c = r.center();
            let radius = r.width() / 2.0 - 1.0;
            let end = 1.35 * PI;
            p.add(egui::Shape::line(arc(c, radius, -0.25 * PI, end), stroke));
            // Arrowhead at the open end, pointing along the arc.
            let at = c + Vec2::angled(end) * radius;
            let along = Vec2::angled(end + PI / 2.0);
            let across = Vec2::angled(end);
            p.add(egui::Shape::convex_polygon(
                vec![
                    at + along * 3.0,
                    at - along * 1.0 + across * 3.0,
                    at - along * 1.0 - across * 3.0,
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Record(on) => {
            let radius = r.width() / 2.0 - 1.0;
            if on {
                p.circle_filled(r.center(), radius, Color32::from_rgb(235, 70, 70));
            } else {
                p.circle_stroke(r.center(), radius, stroke);
                p.circle_filled(r.center(), radius * 0.35, color);
            }
        }
    }
}

impl eframe::App for Overlay {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.engine.is_stopping() {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }
        ctx.request_repaint_after(Duration::from_millis(250));
        // Viewport coordinates include egui zoom; persist screen logical pixels instead.
        let applied_scale = ctx.zoom_factor();
        let desired = self.engine.overlay.read().position;
        if desired != self.last_position {
            if let Some(p) = desired {
                ctx.send_viewport_cmd(ViewportCommand::OuterPosition(Pos2::new(
                    p[0] / applied_scale,
                    p[1] / applied_scale,
                )));
            }
            self.last_position = desired;
        } else if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
            let position = [
                (rect.min.x * applied_scale).round(),
                (rect.min.y * applied_scale).round(),
            ];
            if !ctx.input(|i| i.pointer.any_down()) && self.last_position != Some(position) {
                let _ = self.engine.modify_overlay(|s| s.position = Some(position));
                self.last_position = Some(position);
            }
        }
        let mut live = self.engine.live();
        #[cfg(debug_assertions)]
        if let Some(fixture) = &self.fixture {
            live = fixture.clone();
        }
        live.overlay = self.engine.overlay.read().clone();
        let settings = live.overlay.clone();
        ctx.set_zoom_factor(settings.scale.clamp(0.6, 2.5));

        let passthrough = settings.locked || !settings.visible;
        if self.passthrough != Some(passthrough) {
            ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(passthrough));
            self.passthrough = Some(passthrough);
        }

        let rows = live.rows.len().min(settings.max_rows);
        let footer = footer_visible(&live);
        let size = overlay_size(settings.compact, rows, footer);
        let width = size.x;
        if size != self.last_size || applied_scale != self.last_scale {
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
            self.last_size = size;
            self.last_scale = applied_scale;
        }

        let bg = (settings.opacity.clamp(0.0, 1.0) * 200.0) as u8;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                if !settings.visible {
                    return;
                }
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let full = Rect::from_min_size(Pos2::ZERO, size);
                ui.painter().rect_filled(
                    full,
                    theme(&settings.theme).radius,
                    with_alpha(theme(&settings.theme).ground, bg),
                );
                self.header(ui, &live, width, bg);
                if live.rows.is_empty() {
                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(width, row_height(settings.compact)),
                        Sense::hover(),
                    );
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        "Warte auf Kampf …",
                        FontId::proportional(12.0),
                        Color32::from_gray(170),
                    );
                } else {
                    self.rows(
                        ui,
                        &live,
                        width,
                        settings.max_rows,
                        settings.show_dps,
                        settings.hide_names,
                    );
                }
                ui.add_space(4.0);
                if footer {
                    self.footer(ui, &live, width, bg);
                }
            });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn compact_geometry_keeps_a_single_window_small() {
        assert_eq!(
            super::overlay_size(true, 0, true),
            eframe::egui::vec2(312.0, 86.0)
        );
        assert_eq!(
            super::overlay_size(true, 5, true),
            eframe::egui::vec2(312.0, 198.0)
        );
        assert_eq!(
            super::overlay_size(false, 5, true),
            eframe::egui::vec2(360.0, 266.0)
        );
        // Locked combat mode without alarms drops the footer.
        assert_eq!(
            super::overlay_size(true, 5, false),
            eframe::egui::vec2(312.0, 180.0)
        );
    }

    #[test]
    fn footer_only_while_unlocked_or_when_something_needs_attention() {
        let mut live = crate::engine::Live::default();
        live.capture.permission = true;
        live.capture.locked_port = Some(1);
        assert!(super::footer_visible(&live), "unlocked: setup mode");
        live.overlay.locked = true;
        assert!(
            !super::footer_visible(&live),
            "locked and healthy: no footer"
        );
        live.numeric_limited = true;
        assert!(super::footer_visible(&live));
        live.numeric_limited = false;
        live.capture.recording = Some("x".into());
        assert!(super::footer_visible(&live));
        live.capture.recording = None;
        live.capture.locked_port = None;
        assert!(super::footer_visible(&live));
        live.capture.locked_port = Some(1);
        live.capture.permission = false;
        assert!(super::footer_visible(&live));
    }

    #[test]
    fn masks_names() {
        assert_eq!(super::masked("Mirco"), "M***o");
        assert_eq!(super::masked("Al"), "**");
        assert_eq!(super::masked("A"), "*");
        assert_eq!(super::masked(""), "");
    }
}
