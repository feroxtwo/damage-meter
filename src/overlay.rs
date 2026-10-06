//! The in-game overlay: a small borderless, transparent, always-on-top window.
//!
//! On KDE Plasma (Wayland) KWin decides what stays on top, so the install
//! script adds a window rule for the app id `aion2-meter` that keeps it above
//! the game (see `scripts/install-kwin-rule.sh`). Under XWayland
//! (`--x11`) the always-on-top request works without a rule.

use std::sync::Arc;
use std::time::Duration;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2, ViewportCommand,
};

use crate::engine::{Engine, Live};
use crate::names::{duration, short_number};

pub const APP_ID: &str = "aion2-meter";
const WIDTH: f32 = 330.0;
const HEADER: f32 = 40.0;
const ROW: f32 = 22.0;
const FOOTER: f32 = 18.0;

pub struct Overlay {
    engine: Arc<Engine>,
    web_url: String,
    passthrough: Option<bool>,
    last_size: Vec2,
}

fn masked(name: &str) -> String {
    let chars: Vec<char> = name.chars().collect();
    match chars.len() {
        0..=2 => name.to_string(),
        n => format!("{}{}{}", chars[0], "*".repeat((n - 2).min(4)), chars[n - 1]),
    }
}

fn with_alpha(c: [u8; 3], a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c[0], c[1], c[2], a)
}

impl Overlay {
    pub fn new(engine: Arc<Engine>, web_url: String) -> Self {
        Self { engine, web_url, passthrough: None, last_size: Vec2::ZERO }
    }

    pub fn run(self) -> eframe::Result {
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("AION2 Meter")
                .with_app_id(APP_ID)
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
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, HEADER), Sense::click_and_drag());
        let p = ui.painter();
        p.rect_filled(rect, CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 }, Color32::from_black_alpha(bg.saturating_add(40)));

        let title = if live.target_name.is_empty() { "Kein Ziel".to_string() } else { live.target_name.clone() };
        p.text(
            rect.left_top() + Vec2::new(8.0, 4.0),
            Align2::LEFT_TOP,
            title,
            FontId::proportional(13.5),
            Color32::from_rgb(240, 220, 170),
        );
        let time = duration(live.battle_time_ms);
        let dps_total = if live.battle_time_ms > 0 { live.total_damage * 1000.0 / live.battle_time_ms as f64 } else { 0.0 };
        p.text(
            rect.right_top() + Vec2::new(-8.0, 4.0),
            Align2::RIGHT_TOP,
            format!("{time}  ·  {}/s", short_number(dps_total)),
            FontId::monospace(12.0),
            Color32::from_gray(210),
        );

        // Boss HP
        let bar = Rect::from_min_size(rect.left_top() + Vec2::new(8.0, 24.0), Vec2::new(width - 16.0, 8.0));
        p.rect_filled(bar, 3, Color32::from_black_alpha(160));
        if let Some(hp) = live.target_hp {
            let mut fill = bar;
            fill.set_width(bar.width() * hp as f32);
            p.rect_filled(fill, 3, Color32::from_rgb(196, 52, 52));
            p.text(
                bar.center(),
                Align2::CENTER_CENTER,
                format!("{:.1}%", hp * 100.0),
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
            self.engine.overlay.write().locked = true;
            ui.close();
        }
        let recording = self.engine.recording_wanted();
        if ui.button(if recording { "Mitschnitt stoppen" } else { "Pakete mitschneiden" }).clicked() {
            self.engine.toggle_recording();
            ui.close();
        }
        ui.separator();
        ui.label("Ziel");
        for (id, label) in [
            ("bossTargets", "Bosse"),
            ("mostDamage", "Meister Schaden"),
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
            let _ = std::process::Command::new("xdg-open").arg(&self.web_url).spawn();
            ui.close();
        }
        if ui.button("Beenden").clicked() {
            std::process::exit(0);
        }
    }

    fn rows(&self, ui: &mut egui::Ui, live: &Live, width: f32, max_rows: usize, show_dps: bool, hide_names: bool) {
        let top = live.rows.first().map(|r| r.damage).unwrap_or(1.0).max(1.0);
        for (i, row) in live.rows.iter().take(max_rows).enumerate() {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, ROW), Sense::hover());
            let p = ui.painter();
            let inner = rect.shrink2(Vec2::new(4.0, 1.5));
            p.rect_filled(inner, 3, Color32::from_black_alpha(90));
            let mut fill = inner;
            fill.set_width(inner.width() * (row.damage / top) as f32);
            p.rect_filled(fill, 3, with_alpha(row.color, 170));
            if row.is_self {
                p.rect_stroke(inner, 3, Stroke::new(1.0_f32, Color32::from_rgb(255, 215, 120)), StrokeKind::Inside);
            }
            let name = if hide_names && !row.is_self { masked(&row.name) } else { row.name.clone() };
            let font = FontId::proportional(12.5);
            p.text(
                inner.left_center() + Vec2::new(6.0, 0.0),
                Align2::LEFT_CENTER,
                format!("{}. {}{}", i + 1, if row.dead { "† " } else { "" }, name),
                font.clone(),
                if row.dead { Color32::from_gray(170) } else { Color32::WHITE },
            );
            let right = if show_dps {
                format!("{}  {}/s  {:>3.0}%", short_number(row.damage), short_number(row.dps), row.share)
            } else {
                format!("{}  {:>3.0}%", short_number(row.damage), row.share)
            };
            p.text(inner.right_center() - Vec2::new(6.0, 0.0), Align2::RIGHT_CENTER, right, FontId::monospace(11.5), Color32::WHITE);
        }
    }

    fn footer(&self, ui: &mut egui::Ui, live: &Live, width: f32, bg: u8) {
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, FOOTER), Sense::click());
        let p = ui.painter();
        p.rect_filled(rect, CornerRadius { nw: 0, ne: 0, sw: 8, se: 8 }, Color32::from_black_alpha(bg.saturating_add(40)));
        let c = &live.capture;
        let (dot, state) = if !c.permission {
            (Color32::from_rgb(220, 60, 60), "keine Capture-Berechtigung".to_string())
        } else if c.locked_port.is_some() {
            (Color32::from_rgb(80, 200, 100), live.dungeon.clone())
        } else if c.game_running {
            (Color32::from_rgb(230, 180, 60), "suche Verbindung …".to_string())
        } else {
            (Color32::from_gray(140), "AION2 nicht gestartet".to_string())
        };
        p.circle_filled(rect.left_center() + Vec2::new(10.0, 0.0), 3.5, dot);
        p.text(rect.left_center() + Vec2::new(18.0, 0.0), Align2::LEFT_CENTER, state, FontId::proportional(10.5), Color32::from_gray(200));
        let mut ping = live.ping_ms.map(|p| format!("{p} ms")).unwrap_or_default();
        if c.recording.is_some() {
            ping = format!("● REC  {ping}");
        }
        p.text(rect.right_center() - Vec2::new(8.0, 0.0), Align2::RIGHT_CENTER, ping, FontId::monospace(10.5), Color32::from_gray(200));
        resp.context_menu(|ui| self.menu(ui));
    }
}

impl eframe::App for Overlay {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(250));
        let live = self.engine.live();
        let settings = live.overlay.clone();
        ctx.set_zoom_factor(settings.scale.clamp(0.6, 2.5));

        let passthrough = settings.locked || !settings.visible;
        if self.passthrough != Some(passthrough) {
            ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(passthrough));
            self.passthrough = Some(passthrough);
        }

        let rows = live.rows.len().min(settings.max_rows);
        let size = Vec2::new(WIDTH, HEADER + FOOTER + ROW * rows.max(1) as f32 + 4.0);
        if size != self.last_size {
            ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
            self.last_size = size;
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
                ui.painter().rect_filled(full, 8, Color32::from_black_alpha(bg));
                self.header(ui, &live, WIDTH, bg);
                if live.rows.is_empty() {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(WIDTH, ROW), Sense::hover());
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        "Warte auf Kampf …",
                        FontId::proportional(12.0),
                        Color32::from_gray(170),
                    );
                } else {
                    self.rows(ui, &live, WIDTH, settings.max_rows, settings.show_dps, settings.hide_names);
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
        assert_eq!(super::masked("Al"), "Al");
    }
}
