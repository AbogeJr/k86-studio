//! Backlight effects and the per-key custom pattern editor.

use eframe::egui::{self, Color32, RichText};

use super::keyboard::{key_slot, KeyView};
use super::Ctx;
use crate::device::{Command, Snapshot};
use crate::proto::{LightMode, LightSetting, Rgb, SLOTS};
use crate::theme;

struct Effect {
    mode: u8,
    name: &'static str,
    color: bool,
    speed: bool,
    options: &'static [&'static str],
}

/// The K86's effect list (`Be` in the original), in display order.
const EFFECTS: &[Effect] = &[
    Effect { mode: 4, name: "Wave", color: true, speed: true, options: &["Right", "Left", "Down", "Up"] },
    Effect { mode: 5, name: "Ripple", color: true, speed: true, options: &[] },
    Effect { mode: 6, name: "Starlight", color: true, speed: true, options: &[] },
    Effect { mode: 7, name: "Stream", color: true, speed: true, options: &["Z-shape", "Round"] },
    Effect { mode: 8, name: "Shadow", color: true, speed: true, options: &[] },
    Effect { mode: 9, name: "Mountain Wave", color: true, speed: true, options: &[] },
    Effect { mode: 10, name: "Sine Wave", color: true, speed: true, options: &[] },
    Effect { mode: 11, name: "Color Spring", color: true, speed: true, options: &["Outward", "Inward"] },
    Effect { mode: 19, name: "Snow Trace", color: true, speed: true, options: &[] },
    Effect { mode: 12, name: "Flower Wave", color: true, speed: true, options: &["Right", "Left"] },
    Effect { mode: 14, name: "Kill Two Birds", color: true, speed: true, options: &[] },
    Effect { mode: 15, name: "Circle Wave", color: true, speed: true, options: &["Counterclockwise", "Clockwise"] },
    Effect { mode: 16, name: "Colorful Cross", color: true, speed: true, options: &[] },
    Effect { mode: 17, name: "Snow Fall", color: true, speed: true, options: &[] },
    Effect { mode: 18, name: "Meteor", color: true, speed: true, options: &[] },
    Effect { mode: 1, name: "Always On", color: true, speed: false, options: &[] },
    Effect { mode: 2, name: "Breathing", color: true, speed: true, options: &[] },
    Effect { mode: 3, name: "Spectrum Cycle", color: false, speed: true, options: &[] },
    Effect { mode: 13, name: "Custom", color: false, speed: false, options: &["Pattern 1", "Pattern 2", "Pattern 3"] },
];

const MAX_BRIGHT: u8 = 4;
const MAX_SPEED: u8 = 4;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Effects,
    Custom,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tool {
    #[default]
    Pen,
    Picker,
}

pub struct LightingPage {
    tab: Tab,
    draft: Option<LightSetting>,
    picture: Vec<Rgb>,
    pen: Color32,
    random: bool,
    tool: Tool,
    pattern: u8,
    painted_frame: Vec<usize>,
    /// Last edit time; writes go out once the controls settle.
    pending: Option<std::time::Instant>,
}

impl Default for LightingPage {
    fn default() -> Self {
        Self {
            tab: Tab::Effects,
            draft: None,
            picture: vec![Rgb::default(); SLOTS],
            pen: Color32::from_rgb(0x71, 0x84, 0xF1),
            random: false,
            tool: Tool::Pen,
            pattern: 0,
            painted_frame: Vec::new(),
            pending: None,
        }
    }
}

fn rand_rgb() -> Rgb {
    // Cheap xorshift seeded from the clock; quality is irrelevant here.
    let mut x = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1)
        | 1;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    Rgb::from_u32((x as u32) & 0xFF_FFFF)
}

impl LightingPage {
    pub fn sync(&mut self, s: &Snapshot) {
        if self.pending.is_none() {
            self.draft = Some(s.light);
        }
        if s.light.mode == LightMode::USER_PICTURE {
            self.pattern = s.light.option.min(2);
        }
    }

    pub fn set_picture(&mut self, c: Vec<Rgb>) {
        self.picture = c;
        self.picture.resize(SLOTS, Rgb::default());
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, cx: &mut Ctx) {
        super::header(ui, "02", "Lighting", "Backlight effects, colour, brightness and speed. Paint your own pattern per key.");
        ui.horizontal(|ui| {
            if theme::chip(ui, self.tab == Tab::Effects, "Effects").clicked() {
                self.tab = Tab::Effects;
            }
            if theme::chip(ui, self.tab == Tab::Custom, "Custom pattern").clicked() {
                self.tab = Tab::Custom;
            }
        });
        ui.add_space(14.0);
        match self.tab {
            Tab::Effects => self.effects(ui, cx),
            Tab::Custom => self.custom(ui, cx),
        }
    }

    fn effects(&mut self, ui: &mut egui::Ui, cx: &mut Ctx) {
        let mut l = self.draft.unwrap_or(cx.snap.light);
        let before = l;
        let effect = EFFECTS.iter().find(|e| e.mode == l.mode.0);

        ui.columns(2, |cols| {
            theme::card(&mut cols[0], "Effect", |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                    for e in EFFECTS {
                        let sel = l.mode.0 == e.mode;
                        if ui.add(egui::Button::new(e.name).selected(sel).min_size(egui::vec2(132.0, 36.0))).clicked() && !sel {
                            l.mode = LightMode(e.mode);
                            l.option = 0;
                            if !e.color {
                                l.dazzle = false;
                            }
                        }
                    }
                });
                if effect.is_none() {
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new(format!("The keyboard is set to an effect this app doesn't list ({}).", l.mode.key()))
                            .color(theme::AMBER),
                    );
                }
            });
            let effect = EFFECTS.iter().find(|e| e.mode == l.mode.0);
            theme::card(&mut cols[1], "Adjust", |ui| {
                let color = effect.is_some_and(|e| e.color);
                ui.label(theme::label("Colour"));
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(color, |ui| {
                        if theme::chip(ui, color && l.dazzle, "Shining").clicked() {
                            l.dazzle = true;
                        }
                        if theme::chip(ui, color && !l.dazzle, "Custom colour").clicked() {
                            l.dazzle = false;
                        }
                        if !l.dazzle {
                            let mut c = [l.rgb.r, l.rgb.g, l.rgb.b];
                            if ui.color_edit_button_srgb(&mut c).changed() {
                                l.rgb = Rgb::new(c[0], c[1], c[2]);
                            }
                        }
                    });
                });
                ui.add_space(12.0);
                ui.label(theme::label("Brightness"));
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Dark").small().color(theme::TEXT_FAINT));
                    let mut b = l.bright.min(MAX_BRIGHT);
                    if ui.add(egui::Slider::new(&mut b, 0..=MAX_BRIGHT).show_value(false)).changed() {
                        l.bright = b;
                    }
                    ui.label(RichText::new("Bright").small().color(theme::TEXT_FAINT));
                });
                ui.add_space(12.0);
                let speed = effect.is_some_and(|e| e.speed);
                ui.label(theme::label("Speed"));
                ui.add_enabled_ui(speed, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Slow").small().color(theme::TEXT_FAINT));
                        let mut s = l.speed.min(MAX_SPEED);
                        if ui.add(egui::Slider::new(&mut s, 0..=MAX_SPEED).show_value(false)).changed() {
                            l.speed = s;
                        }
                        ui.label(RichText::new("Fast").small().color(theme::TEXT_FAINT));
                    });
                });
                ui.add_space(12.0);
                ui.label(theme::label("Play mode"));
                let opts = effect.map_or(&[][..], |e| e.options);
                ui.add_enabled_ui(!opts.is_empty(), |ui| {
                    let current = opts.get(l.option as usize).copied().unwrap_or("None");
                    egui::ComboBox::from_id_salt("light-option").selected_text(current).show_ui(ui, |ui| {
                        for (i, o) in opts.iter().enumerate() {
                            ui.selectable_value(&mut l.option, i as u8, *o);
                        }
                    });
                });
            });
        });

        self.draft = Some(l);
        if l != before {
            self.pending = Some(std::time::Instant::now());
        }
        let settled = self.pending.is_some_and(|t| t.elapsed().as_millis() > 200) && !ui.input(|i| i.pointer.any_down());
        if settled && !cx.busy {
            self.pending = None;
            cx.snap.light = l;
            cx.send(Command::SetLight(l));
        } else if self.pending.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    fn custom(&mut self, ui: &mut egui::Ui, cx: &mut Ctx) {
        ui.horizontal(|ui| {
            ui.label(theme::label("Pen"));
            ui.color_edit_button_srgba(&mut self.pen);
            if theme::chip(ui, self.tool == Tool::Picker, "Eyedropper").clicked() {
                self.tool = if self.tool == Tool::Picker { Tool::Pen } else { Tool::Picker };
            }
            if theme::chip(ui, self.random, "Random colour").clicked() {
                self.random = !self.random;
            }
            if ui.button("Clear").clicked() {
                self.picture.iter_mut().for_each(|c| *c = Rgb::default());
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if theme::primary_button(ui, "Write").clicked() && !cx.busy {
                    cx.send(Command::WriteUserPicture { pattern: self.pattern, colors: self.picture.clone() });
                }
                if theme::ghost_button(ui, "Read").clicked() && !cx.busy {
                    cx.send(Command::ReadUserPicture { pattern: self.pattern });
                }
                for p in (0..3u8).rev() {
                    if theme::chip(ui, self.pattern == p, &format!("{}", p + 1)).clicked() {
                        self.pattern = p;
                    }
                }
                ui.label(theme::label("Pattern"));
            });
        });
        ui.add_space(12.0);

        let pic = self.picture.clone();
        let fill = |i: usize| {
            key_slot(i).and_then(|s| pic.get(s)).map(|c| Color32::from_rgb(c.r, c.g, c.b)).filter(|c| *c != Color32::BLACK)
        };
        let none = |_: usize| None;
        let any = |i: usize| key_slot(i).is_some();
        let r = KeyView { caption: &none, fill: &fill, selected: &[], enabled: &any }.show(ui);

        if self.tool == Tool::Picker {
            if let Some(slot) = r.clicked.and_then(key_slot) {
                let c = self.picture[slot];
                self.pen = Color32::from_rgb(c.r, c.g, c.b);
                self.tool = Tool::Pen;
            }
        } else {
            if !ui.input(|i| i.pointer.primary_down()) {
                self.painted_frame.clear();
            }
            for i in r.painted.into_iter().chain(r.clicked) {
                if self.painted_frame.contains(&i) {
                    continue;
                }
                self.painted_frame.push(i);
                if let Some(slot) = key_slot(i) {
                    self.picture[slot] = if self.random { rand_rgb() } else { Rgb::new(self.pen.r(), self.pen.g(), self.pen.b()) };
                }
            }
        }
        ui.add_space(10.0);
        ui.label(
            RichText::new("Click or drag across keys to paint. Write switches the keyboard to the chosen pattern.")
                .color(theme::TEXT_FAINT),
        );
    }
}
