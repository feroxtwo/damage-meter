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
const ROW: f32 = 30.0;
fn row_height(compact: bool) -> f32 {
    if compact { 22.0 } else { ROW }
}
fn overlay_width(compact: bool) -> f32 {
    if compact { 312.0 } else { WIDTH }
}
fn header_height(compact: bool) -> f32 {
    if compact { 36.0 } else { HEADER }
}
fn footer_height(compact: bool) -> f32 {
    if compact { 20.0 } else { FOOTER }
}
fn overlay_size(compact: bool, rows: usize) -> Vec2 {
    Vec2::new(
        overlay_width(compact),
        header_height(compact) + footer_height(compact) + row_height(compact) * rows.max(1) as f32 + 4.0,
    )
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

/// A short shadow keeps foreground text readable over bright game scenes.
fn shadow_text(p: &egui::Painter, pos: Pos2, align: Align2, text: impl ToString, font: FontId, color: Color32) {
    let text = text.to_string();
    p.text(pos + Vec2::new(0.0, 1.0), align, &text, font.clone(), Color32::from_black_alpha(240));
    p.text(pos, align, text, font, color);
}

// Debug-only visual QA input; no packet/API injection and absent from release builds.
#[cfg(debug_assertions)]
fn native_fixture() -> Option<Live> {
    let path = std::env::var_os("A2M_NATIVE_FIXTURE")?;
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() > 1024 * 1024 { return None; }
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let mut live = Live { target_name: "SYNTHETISCH · Kargos".into(), target_hp: Some(0.38), battle_time_ms: 90_000, ping_ms: Some(42), ..Default::default() };
    live.capture.permission = true;
    live.capture.game_running = true;
    live.capture.locked_port = Some(13328);
    for (i, r) in v.get("rows")?.as_array()?.iter().take(24).enumerate() {
        let class = crate::names::class_info(r.get("job")?.as_str()?);
        let damage = r.get("damage")?.as_f64()?.clamp(0.0, 1e15);
        live.rows.push(crate::engine::LiveRow {
            id: i as i32 + 1, name: r.get("name")?.as_str()?.chars().take(100).collect(), class_key: class.key, class_name: class.name, color: class.color,
            damage, dps: damage / 90.0, heal: damage / 5.0, hps: damage / 450.0, is_self: i == 0, dead: i == 3, ..Default::default()
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
                .with_min_inner_size([200.0, 60.0])
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
        let (rect, resp) = ui.allocate_exact_size(
            Vec2::new(width, header_height(settings.compact)),
            Sense::click_and_drag(),
        );
        let p = ui.painter();
        p.rect_filled(rect, 6, with_alpha(palette(&settings.theme).1, bg.saturating_add(40)));
        let title = if live.target_name.is_empty() { "Kein Ziel" } else { &live.target_name };
        let time = duration(live.battle_time_ms);
        let mut title_rect = rect;
        title_rect.set_right(rect.right() - 50.0);
        shadow_text(&p.with_clip_rect(title_rect), rect.left_top() + Vec2::new(8.0, 3.0), Align2::LEFT_TOP, title, FontId::proportional(12.5), with_alpha(palette(&settings.theme).2, 255));
        shadow_text(p, rect.right_top() + Vec2::new(-8.0, 3.0), Align2::RIGHT_TOP, time, FontId::monospace(12.0), Color32::WHITE);
        let total = live.rows.iter().map(|r| metric_value(r, &settings.metric)).sum::<f64>();
        let rate = total * 1000.0 / live.battle_time_ms.max(1000) as f64;
        let label = match settings.metric.as_str() { "heal" => "HPS", "damage_received" => "Erlitten/s", _ => "DPS" };
        shadow_text(p, rect.left_top() + Vec2::new(8.0, 19.0), Align2::LEFT_TOP, format!("Gruppe  {} {label}", short_number(rate)), FontId::proportional(10.0), Color32::from_gray(205));
        if let Some(hp) = live.target_hp {
            shadow_text(p, rect.right_top() + Vec2::new(-8.0, 19.0), Align2::RIGHT_TOP, format!("{}{:0.1}% HP", if live.hp_estimated { "~" } else { "" }, hp * 100.0), FontId::proportional(10.0), Color32::from_gray(220));
            let bar = Rect::from_min_size(rect.left_bottom() + Vec2::new(8.0, -3.0), Vec2::new(width - 16.0, 2.0));
            p.rect_filled(bar, 1, Color32::from_black_alpha(160));
            let mut fill = bar;
            fill.set_width(bar.width() * hp.clamp(0.0, 1.0) as f32);
            p.rect_filled(fill, 1, Color32::from_rgb(238, 104, 116));
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
            let inner = rect.shrink2(Vec2::new(4.0, 1.0));
            p.rect_filled(inner, 3, Color32::from_black_alpha(if row.is_self { 155 } else { 110 }));
            let track = Rect::from_min_size(inner.left_bottom() - Vec2::new(0.0, 2.0), Vec2::new(inner.width(), 2.0));
            p.rect_filled(track, 1, Color32::from_black_alpha(160));
            let mut fill = track;
            fill.set_width(track.width() * (value / top).clamp(0.0, 1.0) as f32);
            p.rect_filled(fill, 1, with_alpha(row.color, if row.dead { 100 } else { 220 }));
            if row.is_self {
                p.rect_stroke(inner, 3, Stroke::new(0.7, with_alpha(palette(&settings.theme).2, 140)), StrokeKind::Inside);
            }
            let name = if hide_names && !row.is_self { masked(&row.name) } else { row.name.clone() };
            let primary = if show_dps { format!("{}/s", short_number(rate)) } else { short_number(value) };
            let share = format!("{:0.0}%", value * 100.0 / total);
            let primary_font = FontId::proportional(13.0);
            let primary_width = p.layout_no_wrap(primary.clone(), primary_font.clone(), Color32::WHITE).size().x;
            let primary_x = inner.right() - if settings.compact { 35.0 } else { 6.0 };
            let mut name_rect = inner;
            name_rect.set_right((primary_x - primary_width - 8.0).max(inner.left()));
            let color = if row.dead { Color32::from_gray(155) } else { Color32::WHITE };
            let name_painter = p.with_clip_rect(name_rect);
            shadow_text(&name_painter, inner.left_center() + Vec2::new(5.0, -1.0), Align2::LEFT_CENTER, (i + 1).to_string(), FontId::monospace(10.0), Color32::from_gray(190));
            let icon = self.class_icons.get(row.class_key);
            if let Some(texture) = icon {
                let icon_rect = Rect::from_center_size(inner.left_center() + Vec2::new(29.0, -1.0), Vec2::splat(if settings.compact { 16.0 } else { 18.0 }));
                name_painter.image(texture.id(), icon_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), color);
            }
            shadow_text(&name_painter, inner.left_center() + Vec2::new(if icon.is_some() { 41.0 } else { 22.0 }, -1.0), Align2::LEFT_CENTER, format!("{}{}", if row.dead { "† " } else { "" }, name), FontId::proportional(12.0), color);
            let y = if settings.compact { inner.center().y - 1.0 } else { inner.top() + 7.0 };
            shadow_text(p, Pos2::new(primary_x, y), Align2::RIGHT_CENTER, primary, primary_font, color);
            if settings.compact {
                shadow_text(p, inner.right_center() + Vec2::new(-5.0, -1.0), Align2::RIGHT_CENTER, share, FontId::proportional(9.0), Color32::from_gray(185));
            } else {
                let secondary = if show_dps { format!("{}  ·  {share}", short_number(value)) } else { share };
                shadow_text(p, inner.right_bottom() + Vec2::new(-6.0, -3.0), Align2::RIGHT_BOTTOM, secondary, FontId::proportional(9.0), Color32::from_gray(185));
            }
        }
    }

    fn footer(&self, ui: &mut egui::Ui, live: &Live, width: f32, bg: u8) {
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, footer_height(live.overlay.compact)), Sense::click());
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
            (Color32::from_rgb(80, 200, 100), if live.overlay.compact || live.dungeon.is_empty() { "Verbunden".to_string() } else { live.dungeon.clone() })
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
            state.clone(),
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
        if let Some(fixture) = &self.fixture { live = fixture.clone(); }
        live.overlay = self.engine.overlay.read().clone();
        let settings = live.overlay.clone();
        ctx.set_zoom_factor(settings.scale.clamp(0.6, 2.5));

        let passthrough = settings.locked || !settings.visible;
        if self.passthrough != Some(passthrough) {
            ctx.send_viewport_cmd(ViewportCommand::MousePassthrough(passthrough));
            self.passthrough = Some(passthrough);
        }

        let rows = live.rows.len().min(settings.max_rows);
        let size = overlay_size(settings.compact, rows);
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
                ui.painter()
                    .rect_filled(full, 8, with_alpha(palette(&settings.theme).0, bg));
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
                self.footer(ui, &live, width, bg);
            });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn compact_geometry_keeps_a_single_window_small() {
        assert_eq!(super::overlay_size(true, 0), eframe::egui::vec2(312.0, 82.0));
        assert_eq!(super::overlay_size(true, 5), eframe::egui::vec2(312.0, 170.0));
        assert_eq!(super::overlay_size(false, 5), eframe::egui::vec2(360.0, 216.0));
    }

    #[test]
    fn masks_names() {
        assert_eq!(super::masked("Mirco"), "M***o");
        assert_eq!(super::masked("Al"), "**");
        assert_eq!(super::masked("A"), "*");
        assert_eq!(super::masked(""), "");
    }
}
