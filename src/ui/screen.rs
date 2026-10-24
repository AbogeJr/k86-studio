//! 240x135 screen editor: import, paint, animate and upload.

use std::path::PathBuf;
use std::time::Instant;

use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, TextureHandle, Vec2};
use image::{Rgba, RgbaImage};

use super::Ctx;
use crate::device::Command;
use crate::screen::{self, Frame, H, MAX_FRAMES, W};
use crate::theme;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tool {
    #[default]
    Pen,
    Eraser,
    Picker,
}

struct Import {
    src: screen::Source,
    offset: (f32, f32),
    scale: f32,
    preview: Option<TextureHandle>,
    dirty: bool,
}

pub struct ScreenPage {
    frames: Vec<Frame>,
    current: usize,
    undo: Vec<(usize, Frame)>,
    redo: Vec<(usize, Frame)>,
    tool: Tool,
    brush: u8,
    color: Color32,
    slot: u8,
    tex: Option<TextureHandle>,
    tex_dirty: bool,
    playing: Option<Instant>,
    import: Option<Import>,
    stroke_open: bool,
    last_px: Option<(i32, i32)>,
    confirm_erase: bool,
    error: Option<String>,
    /// Frame interval taken from an imported GIF, applied to the store next frame.
    imported_delay: Option<u8>,
}

impl Default for ScreenPage {
    fn default() -> Self {
        Self {
            frames: vec![screen::blank()],
            current: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            tool: Tool::Pen,
            brush: 0,
            color: Color32::from_rgb(255, 138, 31),
            slot: 0,
            tex: None,
            tex_dirty: true,
            playing: None,
            import: None,
            stroke_open: false,
            last_px: None,
            confirm_erase: false,
            error: None,
            imported_delay: None,
        }
    }
}

fn to_color_image(f: &RgbaImage) -> egui::ColorImage {
    egui::ColorImage::from_rgba_unmultiplied([f.width() as usize, f.height() as usize], f.as_raw())
}

fn pick_file(save: bool, name: &str, exts: &[&str]) -> Option<PathBuf> {
    let d = rfd::FileDialog::new().add_filter("Image", exts);
    if save { d.set_file_name(name).save_file() } else { d.pick_file() }
}

impl ScreenPage {
    pub fn animating(&self) -> bool {
        self.playing.is_some()
    }

    fn checkpoint(&mut self) {
        self.undo.push((self.current, self.frames[self.current].clone()));
        if self.undo.len() > 50 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn paint(&mut self, x: i32, y: i32) {
        let c = match self.tool {
            Tool::Eraser => Rgba([screen::ERASE[0], screen::ERASE[1], screen::ERASE[2], 255]),
            _ => Rgba([self.color.r(), self.color.g(), self.color.b(), 255]),
        };
        let r = self.brush as i32;
        let f = &mut self.frames[self.current];
        for dy in -r..=r {
            for dx in -r..=r {
                let (px, py) = (x + dx, y + dy);
                if px >= 0 && py >= 0 && (px as u32) < W && (py as u32) < H {
                    f.put_pixel(px as u32, py as u32, c);
                }
            }
        }
        self.tex_dirty = true;
    }

    fn line(&mut self, a: (i32, i32), b: (i32, i32)) {
        let steps = (b.0 - a.0).abs().max((b.1 - a.1).abs()).max(1);
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let x = a.0 as f32 + (b.0 - a.0) as f32 * t;
            let y = a.1 as f32 + (b.1 - a.1) as f32 * t;
            self.paint(x.round() as i32, y.round() as i32);
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, cx: &mut Ctx) {
        super::header(ui, "04", "Screen", "Put your own image or animation on the 240×135 display.");
        if let Some(e) = self.error.take() {
            cx.note(e);
        }
        if let Some(d) = self.imported_delay.take() {
            cx.store.screen_delay = d.max(1);
        }

        if self.import.is_some() {
            self.import_ui(ui);
            return;
        }

        // Advance the preview animation.
        let shown = if let Some(start) = self.playing {
            let delay = cx.store.screen_delay.max(10) as u128;
            (start.elapsed().as_millis() / delay) as usize % self.frames.len()
        } else {
            self.current
        };
        if self.playing.is_some() || self.tex_dirty {
            let img = to_color_image(&self.frames[shown]);
            match &mut self.tex {
                Some(t) => t.set(img, egui::TextureOptions::NEAREST),
                None => self.tex = Some(ui.ctx().load_texture("screen", img, egui::TextureOptions::NEAREST)),
            }
            self.tex_dirty = self.playing.is_some();
        }

        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                self.toolbar(ui);
                ui.add_space(8.0);
                self.canvas(ui);
                ui.add_space(10.0);
                self.frames_strip(ui, cx);
            });
            ui.add_space(18.0);
            ui.vertical(|ui| {
                ui.set_width(ui.available_width().min(320.0));
                self.device_panel(ui, cx);
            });
        });

        if self.confirm_erase {
            let mut open = true;
            super::keys::modal(ui.ctx(), "Clear screen", &mut open, |ui| {
                ui.label("Erase every image and animation stored on the keyboard's screen? This cannot be undone and takes about a minute.");
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "Clear screen").clicked() {
                        cx.send(Command::EraseScreen);
                        self.confirm_erase = false;
                    }
                    if theme::ghost_button(ui, "Cancel").clicked() {
                        self.confirm_erase = false;
                    }
                });
            });
            if !open {
                self.confirm_erase = false;
            }
        }
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if theme::ghost_button(ui, "Import…").clicked() {
                if let Some(p) = pick_file(false, "", &["png", "jpg", "jpeg", "gif", "bmp", "webp"]) {
                    match screen::load(&p) {
                        Ok(src) => {
                            let (x, y, _, _) = screen::fit(src.frames[0].width(), src.frames[0].height());
                            self.import = Some(Import { src, offset: (x, y), scale: 1.0, preview: None, dirty: true });
                        }
                        Err(e) => self.error = Some(format!("Could not open image: {e}")),
                    }
                }
            }
            ui.add_space(8.0);
            for (t, n) in [(Tool::Pen, "Pen"), (Tool::Eraser, "Eraser"), (Tool::Picker, "Pick")] {
                if theme::chip(ui, self.tool == t, n).clicked() {
                    self.tool = t;
                }
            }
            ui.add_space(6.0);
            for (b, n) in [(0u8, "1px"), (1, "3px"), (2, "5px")] {
                if theme::chip(ui, self.brush == b, n).clicked() {
                    self.brush = b;
                }
            }
            ui.add_space(6.0);
            ui.color_edit_button_srgba(&mut self.color);
            ui.add_space(6.0);
            let undo = ui.add_enabled(!self.undo.is_empty(), egui::Button::new("↶")).on_hover_text("Undo (Ctrl+Z)");
            let redo = ui.add_enabled(!self.redo.is_empty(), egui::Button::new("↷")).on_hover_text("Redo (Ctrl+Y)");
            let (cz, cy) = ui.input(|i| (i.modifiers.command && i.key_pressed(egui::Key::Z), i.modifiers.command && i.key_pressed(egui::Key::Y)));
            if (undo.clicked() || cz) && !self.undo.is_empty() {
                let (i, f) = self.undo.pop().unwrap();
                if i < self.frames.len() {
                    self.redo.push((i, std::mem::replace(&mut self.frames[i], f)));
                    self.current = i;
                    self.tex_dirty = true;
                }
            }
            if (redo.clicked() || cy) && !self.redo.is_empty() {
                let (i, f) = self.redo.pop().unwrap();
                if i < self.frames.len() {
                    self.undo.push((i, std::mem::replace(&mut self.frames[i], f)));
                    self.current = i;
                    self.tex_dirty = true;
                }
            }
            if ui.button("Clear frame").clicked() {
                self.checkpoint();
                self.frames[self.current] = screen::blank();
                self.tex_dirty = true;
            }
        });
    }

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let scale = ((ui.available_width() - 340.0) / W as f32).clamp(1.5, 4.0).floor();
        let size = Vec2::new(W as f32, H as f32) * scale;
        let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
        if let Some(t) = &self.tex {
            ui.painter().image(t.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        ui.painter().rect_stroke(rect.expand(1.0), 0.0, Stroke::new(1.0, theme::BORDER_STRONG), egui::StrokeKind::Outside);
        if self.playing.is_some() {
            return;
        }
        let to_px = |p: Pos2| (((p.x - rect.min.x) / scale) as i32, ((p.y - rect.min.y) / scale) as i32);
        if let Some(pos) = resp.interact_pointer_pos() {
            let px = to_px(pos);
            if self.tool == Tool::Picker {
                if resp.clicked() && px.0 >= 0 && px.1 >= 0 && (px.0 as u32) < W && (px.1 as u32) < H {
                    let p = self.frames[self.current].get_pixel(px.0 as u32, px.1 as u32);
                    self.color = Color32::from_rgb(p[0], p[1], p[2]);
                    self.tool = Tool::Pen;
                }
            } else if resp.is_pointer_button_down_on() {
                if !self.stroke_open {
                    self.checkpoint();
                    self.stroke_open = true;
                    self.last_px = None;
                }
                match self.last_px {
                    Some(prev) => self.line(prev, px),
                    None => self.paint(px.0, px.1),
                }
                self.last_px = Some(px);
            }
        }
        if !resp.is_pointer_button_down_on() {
            self.stroke_open = false;
            self.last_px = None;
        }
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
    }

    fn frames_strip(&mut self, ui: &mut egui::Ui, cx: &mut Ctx) {
        ui.horizontal(|ui| {
            ui.label(theme::label(&format!("Frames {}/{}", self.current + 1, self.frames.len())));
            ui.add_space(8.0);
            let full = self.frames.len() >= MAX_FRAMES;
            if ui.add_enabled(!full, egui::Button::new("+ Add")).clicked() {
                self.frames.insert(self.current + 1, screen::blank());
                self.current += 1;
                self.tex_dirty = true;
            }
            if ui.add_enabled(!full, egui::Button::new("Duplicate")).clicked() {
                let f = self.frames[self.current].clone();
                self.frames.insert(self.current + 1, f);
                self.current += 1;
                self.tex_dirty = true;
            }
            if ui.add_enabled(self.frames.len() > 1, egui::Button::new("Delete")).clicked() {
                self.frames.remove(self.current);
                self.current = self.current.min(self.frames.len() - 1);
                self.undo.clear();
                self.redo.clear();
                self.tex_dirty = true;
            }
            if ui.button("Delete all").clicked() {
                self.frames = vec![screen::blank()];
                self.current = 0;
                self.undo.clear();
                self.redo.clear();
                self.tex_dirty = true;
            }
            ui.add_space(8.0);
            ui.label(theme::label("Interval"));
            ui.add(egui::DragValue::new(&mut cx.store.screen_delay).range(1..=255).suffix(" ms"));
            let play = self.playing.is_some();
            if ui.add_enabled(self.frames.len() > 1, egui::Button::new(if play { "■ Stop" } else { "▶ Preview" })).clicked() {
                self.playing = if play { None } else { Some(Instant::now()) };
                self.tex_dirty = true;
            }
        });
        ui.add_space(6.0);
        egui::ScrollArea::horizontal().id_salt("frames").max_height(64.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                for i in 0..self.frames.len() {
                    let sel = i == self.current;
                    let b = egui::Button::new(RichText::new(format!("{:02}", i + 1)).font(theme::mono(10.5)))
                        .selected(sel)
                        .min_size(Vec2::new(40.0, 28.0));
                    if ui.add(b).clicked() {
                        self.current = i;
                        self.playing = None;
                        self.tex_dirty = true;
                    }
                }
            });
        });
    }

    fn device_panel(&mut self, ui: &mut egui::Ui, cx: &mut Ctx) {
        theme::card(ui, "Write to keyboard", |ui| {
            ui.label(theme::label("Static slot"));
            ui.horizontal(|ui| {
                for s in 0..screen::SLOTS {
                    if theme::chip(ui, self.slot == s, &format!("{}", s + 1)).clicked() {
                        self.slot = s;
                    }
                }
            });
            ui.add_space(10.0);
            if theme::primary_button(ui, "Write image").on_hover_text("Writes the current frame to the chosen slot").clicked() && !cx.busy {
                cx.send(Command::UploadScreen { frames: vec![self.frames[self.current].clone()], slot: self.slot, delay: 0 });
            }
            ui.add_space(6.0);
            let anim = self.frames.len() > 1;
            if ui.add_enabled_ui(anim, |ui| theme::primary_button(ui, "Write animation")).inner.clicked() && anim && !cx.busy {
                cx.send(Command::UploadScreen { frames: self.frames.clone(), slot: 0, delay: cx.store.screen_delay });
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Uploads need the USB cable (not 2.4G/Bluetooth).").small().color(theme::TEXT_FAINT));
        });
        ui.add_space(12.0);
        theme::card(ui, "Screen", |ui| {
            if theme::ghost_button(ui, "Sync screen time").clicked() {
                cx.send(Command::SyncClock);
            }
            ui.add_space(6.0);
            if theme::danger_button(ui, "Clear screen").clicked() {
                self.confirm_erase = true;
            }
        });
        ui.add_space(12.0);
        theme::card(ui, "Files", |ui| {
            ui.horizontal(|ui| {
                if theme::ghost_button(ui, "Save PNG").clicked() {
                    if let Some(p) = pick_file(true, "screen.png", &["png"]) {
                        if let Err(e) = screen::save_png(&p, &self.frames[self.current]) {
                            self.error = Some(e.to_string());
                        }
                    }
                }
                if ui.add_enabled_ui(self.frames.len() > 1, |ui| theme::ghost_button(ui, "Save GIF")).inner.clicked() {
                    if let Some(p) = pick_file(true, "screen.gif", &["gif"]) {
                        if let Err(e) = screen::save_gif(&p, &self.frames, cx.store.screen_delay as u16) {
                            self.error = Some(e.to_string());
                        }
                    }
                }
            });
        });
    }

    fn import_ui(&mut self, ui: &mut egui::Ui) {
        let Some(mut imp) = self.import.take() else { return };
        let mut keep = true;
        if imp.dirty {
            let f = screen::place(&imp.src.frames[0], imp.offset, imp.scale);
            let img = to_color_image(&f);
            match &mut imp.preview {
                Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                None => imp.preview = Some(ui.ctx().load_texture("import", img, egui::TextureOptions::LINEAR)),
            }
            imp.dirty = false;
        }
        theme::card(ui, "Place image", |ui| {
            ui.label(RichText::new("Drag to move, scroll or use the slider to scale.").color(theme::TEXT_DIM));
            ui.add_space(10.0);
            let scale = 3.0;
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(W as f32, H as f32) * scale, Sense::drag());
            if let Some(t) = &imp.preview {
                ui.painter().image(t.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
            ui.painter().rect_stroke(rect.expand(1.0), 0.0, Stroke::new(1.0, theme::ACCENT), egui::StrokeKind::Outside);
            if resp.dragged() {
                let d = resp.drag_delta() / scale;
                imp.offset.0 += d.x;
                imp.offset.1 += d.y;
                imp.dirty = true;
            }
            if resp.hovered() {
                let s = ui.input(|i| i.smooth_scroll_delta.y);
                if s != 0.0 {
                    imp.scale = (imp.scale + s.signum() * 0.2).clamp(0.2, 3.0);
                    imp.dirty = true;
                }
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(theme::label("Scale"));
                if ui.add(egui::Slider::new(&mut imp.scale, 0.2..=3.0).step_by(0.1)).changed() {
                    imp.dirty = true;
                }
                if ui.button("Fit").clicked() {
                    let (x, y, _, _) = screen::fit(imp.src.frames[0].width(), imp.src.frames[0].height());
                    imp.offset = (x, y);
                    imp.scale = 1.0;
                    imp.dirty = true;
                }
                if imp.src.frames.len() > 1 {
                    ui.label(RichText::new(format!("{} frames", imp.src.frames.len())).color(theme::TEXT_DIM));
                }
            });
            ui.add_space(12.0);
            let mut done = None;
            ui.horizontal(|ui| {
                if theme::primary_button(ui, "Use image").clicked() {
                    done = Some(true);
                }
                if theme::ghost_button(ui, "Cancel").clicked() {
                    done = Some(false);
                }
            });
            if let Some(ok) = done {
                keep = false;
                if ok {
                    let frames: Vec<Frame> = imp.src.frames.iter().map(|f| screen::place(f, imp.offset, imp.scale)).collect();
                    let delay = imp.src.delay_ms;
                    if frames.len() > 1 {
                        self.frames = frames;
                        self.current = 0;
                        self.undo.clear();
                        self.redo.clear();
                        if let Some(d) = delay {
                            self.pending_delay(d);
                        }
                    } else {
                        self.checkpoint();
                        self.frames[self.current] = frames.into_iter().next().unwrap();
                    }
                    self.tex_dirty = true;
                }
            }
        });
        if keep {
            self.import = Some(imp);
        }
    }

    fn pending_delay(&mut self, d: u8) {
        self.imported_delay = Some(d);
    }
}
