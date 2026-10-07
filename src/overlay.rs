//! The in-game overlay: a small borderless, transparent, always-on-top window.
//!
//! On KDE Plasma (Wayland) KWin decides what stays on top, so the install
//! script adds a window rule for the app id `aion2-meter` that keeps it above
//! the game (see `scripts/install-kwin-rule.sh`). Under XWayland
//! (`--x11`) the always-on-top request works without a rule.

use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2,
    ViewportCommand,
};

use crate::engine::{Engine, Live, metric_value, overlay_rows};
use crate::names::{duration, short_number};

pub const APP_ID: &str = "aion2-meter";
const WIDTH: f32 = 360.0;
const HEADER: f32 = 40.0;
const ROW: f32 = 26.0;
fn row_height(compact: bool) -> f32 {
    if compact { 22.0 } else { ROW }
}
fn palette(theme: &str) -> ([u8; 3], [u8; 3], [u8; 3]) {
    match theme {
        "aether" => ([8, 26, 32], [16, 41, 48], [184, 238, 231]),
        "ember" => ([25, 17, 21], [42, 29, 36], [244, 216, 167]),
        _ => ([11, 16, 24], [16, 28, 44], [240, 220, 170]),
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
        }
    }

    pub fn run(self) -> eframe::Result {
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("AION2 Meter")
                .with_app_id(APP_ID)
                .with_position(self.engine.overlay.read().position.unwrap_or([40.0, 40.0]))
                .with_inner_size([WIDTH, HEADER + FOOTER + ROW * 2.0])
                .with_min_inner_size([200.0, 60.0])
                .with_decorations(false)
                .with_transparent(true)
                .with_always_on_top()
                .with_taskbar(false)
                .with_resizable(false),
            ..Default::default()
        };
        eframe::run_native("AION2 Meter", options, Box::new(|_cc| Ok(Box::new(self))))
    }

    fn header(&self, ui: &mut egui::Ui, live: &Live, width: f32, bg: u8) {
        let (rect, resp) =
            ui.allocate_exact_size(Vec2::new(width, HEADER), Sense::click_and_drag());
        let p = ui.painter();
        p.rect_filled(
            rect,
            CornerRadius {
                nw: 8,
                ne: 8,
                sw: 0,
                se: 0,
            },
            with_alpha(
                palette(&self.engine.overlay.read().theme).1,
                bg.saturating_add(40),
            ),
        );

        let mut title = if live.target_name.is_empty() {
            "Kein Ziel".to_string()
        } else {
            live.target_name.clone()
        };
        let metric = self.engine.overlay.read().metric.clone();
        if metric != "damage" {
            title = format!(
                "{} · {}",
                if metric == "heal" {
                    "Heilung"
                } else {
                    "Erlitten"
                },
                title
            );
        }
        let time = duration(live.battle_time_ms);
        let total: f64 = live
            .rows
            .iter()
            .fold(0.0, |sum, r| sum + metric_value(r, &metric));
        let dps_total = total * 1000.0 / live.battle_time_ms.max(1000) as f64;
        let right = format!("{time}  ·  {}/s", short_number(dps_total));
        let right_width = p
            .layout_no_wrap(right.clone(), FontId::monospace(12.0), Color32::WHITE)
            .size()
            .x;
        let mut title_rect = rect;
        title_rect.set_right((rect.right() - right_width - 20.0).max(rect.left()));
        p.with_clip_rect(title_rect).text(
            rect.left_top() + Vec2::new(8.0, 4.0),
            Align2::LEFT_TOP,
            title,
            FontId::proportional(13.5),
            with_alpha(palette(&self.engine.overlay.read().theme).2, 255),
        );
        p.text(
            rect.right_top() + Vec2::new(-8.0, 4.0),
            Align2::RIGHT_TOP,
            right,
            FontId::monospace(12.0),
            Color32::from_gray(210),
        );

        // Boss HP
        let bar = Rect::from_min_size(
            rect.left_top() + Vec2::new(8.0, 24.0),
            Vec2::new(width - 16.0, 8.0),
        );
        p.rect_filled(bar, 3, Color32::from_black_alpha(160));
        if let Some(hp) = live.target_hp {
            let mut fill = bar;
            fill.set_width(bar.width() * hp as f32);
            p.rect_filled(fill, 3, Color32::from_rgb(196, 52, 52));
            p.text(
                bar.center(),
                Align2::CENTER_CENTER,
                format!(
                    "{}{:.1}%",
                    if live.hp_estimated { "~" } else { "" },
                    hp * 100.0
                ),
                FontId::proportional(9.0),
                Color32::WHITE,
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
            let inner = rect.shrink2(Vec2::new(4.0, 1.5));
            p.rect_filled(inner, 3, Color32::from_black_alpha(90));
            let mut fill = inner;
            fill.set_width(inner.width() * (value / top).clamp(0.0, 1.0) as f32);
            p.rect_filled(fill, 3, with_alpha(row.color, 115));
            if row.is_self {
                p.rect_stroke(
                    inner,
                    3,
                    Stroke::new(1.0_f32, Color32::from_rgb(255, 215, 120)),
                    StrokeKind::Inside,
                );
            }
            let name = if hide_names && !row.is_self {
                masked(&row.name)
            } else {
                row.name.clone()
            };
            let right = if show_dps {
                format!(
                    "{}  {}/s  {:>3.0}%",
                    short_number(value),
                    short_number(rate),
                    value * 100.0 / total
                )
            } else {
                format!("{}  {:>3.0}%", short_number(value), value * 100.0 / total)
            };
            let font = FontId::proportional(12.5);
            let mut name_rect = inner;
            let right_width = p
                .layout_no_wrap(right.clone(), FontId::monospace(11.5), Color32::WHITE)
                .size()
                .x;
            name_rect.set_right((inner.right() - right_width - 16.0).max(inner.left()));
            p.with_clip_rect(name_rect).text(
                inner.left_center() + Vec2::new(6.0, 0.0),
                Align2::LEFT_CENTER,
                format!("{}. {}{}", i + 1, if row.dead { "† " } else { "" }, name),
                font.clone(),
                if row.dead {
                    Color32::from_gray(170)
                } else {
                    Color32::WHITE
                },
            );
            p.text(
                inner.right_center() - Vec2::new(6.0, 0.0),
                Align2::RIGHT_CENTER,
                right,
                FontId::monospace(11.5),
                Color32::WHITE,
            );
        }
    }

    fn footer(&self, ui: &mut egui::Ui, live: &Live, width: f32, bg: u8) {
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, FOOTER), Sense::click());
        let p = ui.painter();
        p.rect_filled(
            rect,
            CornerRadius {
                nw: 0,
                ne: 0,
                sw: 8,
                se: 8,
            },
            with_alpha(
                palette(&self.engine.overlay.read().theme).1,
                bg.saturating_add(40),
            ),
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
        } else if c.locked_port.is_some() {
            (Color32::from_rgb(80, 200, 100), live.dungeon.clone())
        } else if c.game_running {
            (
                Color32::from_rgb(230, 180, 60),
                "suche Verbindung …".to_string(),
            )
        } else {
            (Color32::from_gray(140), "AION2 nicht gestartet".to_string())
        };
        p.circle_filled(rect.left_center() + Vec2::new(10.0, 0.0), 3.5, dot);
        let mut state_rect = rect;
        state_rect.set_right(rect.right() - 4.0 * (ICON + 3.0) - 52.0);
        p.with_clip_rect(state_rect).text(
            rect.left_center() + Vec2::new(18.0, 0.0),
            Align2::LEFT_CENTER,
            state,
            FontId::proportional(10.5),
            Color32::from_gray(200),
        );
        // Buttons for what the KDE shortcuts do, right to left.
        let mut x = rect.right() - 4.0;
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

        let ping = live.ping_ms.map(|p| format!("{p} ms")).unwrap_or_default();
        let p = ui.painter();
        p.text(
            Pos2::new(x - 2.0, rect.center().y),
            Align2::RIGHT_CENTER,
            ping,
            FontId::monospace(10.5),
            Color32::from_gray(200),
        );
        resp.context_menu(|ui| self.menu(ui));
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
        live.overlay = self.engine.overlay.read().clone();
        let settings = live.overlay.clone();
        ctx.set_zoom_factor(settings.scale.clamp(0.6, 2.5));

        let passthrough = settings.locked || !settings.visible;
        if self.passthrough != Some(passthrough) {
            ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(passthrough));
            self.passthrough = Some(passthrough);
        }

        let rows = live.rows.len().min(settings.max_rows);
        let size = Vec2::new(
            WIDTH,
            HEADER + FOOTER + row_height(settings.compact) * rows.max(1) as f32 + 4.0,
        );
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
                ui.painter()
                    .rect_filled(full, 8, with_alpha(palette(&settings.theme).0, bg));
                self.header(ui, &live, WIDTH, bg);
                if live.rows.is_empty() {
                    let (rect, _) = ui.allocate_exact_size(
                        Vec2::new(WIDTH, row_height(settings.compact)),
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
                        WIDTH,
                        settings.max_rows,
                        settings.show_dps,
                        settings.hide_names,
                    );
                }
                ui.add_space(4.0);
                self.footer(ui, &live, WIDTH, bg);
            });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn masks_names() {
        assert_eq!(super::masked("Mirco"), "M***o");
        assert_eq!(super::masked("Al"), "**");
        assert_eq!(super::masked("A"), "*");
        assert_eq!(super::masked(""), "");
    }
}
