//! Interactive drawing of the K86 key layout.

use eframe::egui::{self, Align2, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::keymap;
use crate::layout::{self, KEYS};
use crate::theme;

pub struct KeyView<'a> {
    /// Primary caption per key (indexed like `KEYS`).
    pub caption: &'a dyn Fn(usize) -> Option<String>,
    /// Highlight colour per key (e.g. per-key RGB); `None` keeps the default.
    pub fill: &'a dyn Fn(usize) -> Option<Color32>,
    pub selected: &'a [usize],
    /// Keys the user may click.
    pub enabled: &'a dyn Fn(usize) -> bool,
}

/// Matrix slot of each drawn key.
pub fn key_slot(i: usize) -> Option<usize> {
    let hid = KEYS[i].hid;
    let nth = KEYS[..i].iter().filter(|k| k.hid == hid).count();
    keymap::slot_of_hid(hid, nth)
}

pub struct KeyResponse {
    pub clicked: Option<usize>,
    /// Keys swept over while dragging with the primary button.
    pub painted: Vec<usize>,
}

impl KeyView<'_> {
    pub fn show(&self, ui: &mut egui::Ui) -> KeyResponse {
        let (cw, ch) = layout::CANVAS;
        let width = ui.available_width().min(1320.0);
        let scale = width / cw;
        let pad = (ui.available_width() - width) / 2.0;
        let (outer, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ch * scale), Sense::hover());
        let rect = Rect::from_min_size(outer.min + Vec2::new(pad, 0.0), Vec2::new(width, ch * scale));
        let painter = ui.painter_at(rect.expand(2.0));
        let mut out = KeyResponse { clicked: None, painted: Vec::new() };

        // Knob background circle; the knob slices are drawn as keys below.
        let knob_c = rect.min + Vec2::new(814.0, 26.0) * scale;
        painter.circle(knob_c, 26.0 * scale, theme::RAISED, Stroke::new(1.0, theme::BORDER_STRONG));

        for (i, k) in KEYS.iter().enumerate() {
            let r = Rect::from_min_size(rect.min + Vec2::new(k.x, k.y) * scale, Vec2::new(k.w, k.h) * scale);
            let knob = k.x >= 780.0 && k.y < 60.0;
            let enabled = (self.enabled)(i);
            let id = ui.id().with(("key", i));
            let resp = ui.interact(r, id, if enabled { Sense::click_and_drag() } else { Sense::hover() });
            let selected = self.selected.contains(&i);
            let hovered = enabled && resp.hovered();

            let custom = (self.fill)(i);
            let base = if selected {
                theme::RAISED
            } else if hovered {
                theme::HOVER
            } else {
                theme::SURFACE
            };
            let stroke = if selected {
                Stroke::new(1.5, theme::ACCENT)
            } else if hovered {
                Stroke::new(1.0, theme::BORDER_STRONG)
            } else {
                Stroke::new(1.0, theme::BORDER)
            };
            if !knob {
                painter.rect(r, CornerRadius::same(2), base, stroke, StrokeKind::Inside);
                if let Some(c) = custom {
                    // Colour swatch along the bottom edge reads like keycap glow.
                    let strip = Rect::from_min_max(Pos2::new(r.min.x + 3.0, r.max.y - 5.0 * scale.max(0.6)), r.max - Vec2::new(3.0, 3.0));
                    painter.rect_filled(strip, CornerRadius::ZERO, c);
                }
            } else if selected || hovered {
                painter.rect_stroke(r, CornerRadius::ZERO, stroke, StrokeKind::Inside);
            }

            let caption = (self.caption)(i).unwrap_or_else(|| k.label.to_string());
            let changed = (self.caption)(i).is_some();
            let size = (10.5 * scale).clamp(8.0, 12.0);
            let color = if !enabled {
                theme::TEXT_FAINT
            } else if changed {
                theme::ACCENT
            } else {
                theme::TEXT
            };
            let text = if knob {
                match i {
                    _ if k.x < 805.0 => "−".to_string(),
                    _ if k.x < 823.0 => "●".to_string(),
                    _ => "+".to_string(),
                }
            } else {
                caption
            };
            let galley = painter.layout(text, theme::mono(size), color, r.width() - 6.0);
            let pos = if knob { r.center() - galley.size() / 2.0 } else { r.left_top() + Vec2::new(5.0, 4.0) };
            painter.galley(pos, galley, color);
            if changed && !knob && r.height() > 30.0 {
                painter.text(
                    r.right_bottom() - Vec2::new(4.0, 3.0 + if custom.is_some() { 5.0 } else { 0.0 }),
                    Align2::RIGHT_BOTTOM,
                    k.label.split('\n').next().unwrap_or(""),
                    theme::mono((8.0 * scale).clamp(7.0, 9.0)),
                    theme::TEXT_FAINT,
                );
            }

            if resp.clicked() {
                out.clicked = Some(i);
            }
            if enabled && ui.input(|inp| inp.pointer.primary_down()) && resp.contains_pointer() {
                out.painted.push(i);
            }
            if hovered {
                resp.on_hover_cursor(egui::CursorIcon::PointingHand);
            }
        }
        out
    }
}
