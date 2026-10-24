//! Macro library: record, edit, and sync macros that live on the keyboard.

use std::time::Instant;

use eframe::egui::{self, Key, PointerButton, RichText, Sense, Stroke};

use super::labels;
use super::Ctx;
use crate::device::Command;
use crate::keymap::{Macro, MacroEvent, MacroInput, MouseButton};
use crate::store::NamedMacro;
use crate::theme;

#[derive(Default)]
pub struct MacrosPage {
    current: Option<usize>,
    draft: Option<NamedMacro>,
    recording: Option<Recorder>,
    fixed_delay: Option<u16>,
    confirm_delete: bool,
    /// Waiting for a key/click to insert as a press + release pair.
    capture: bool,
}

struct Recorder {
    last: Option<Instant>,
    mods: egui::Modifiers,
}

/// egui key -> HID keyboard usage.
fn hid_of(k: Key) -> Option<u8> {
    use Key::*;
    let letters = [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z];
    if let Some(i) = letters.iter().position(|&l| l == k) {
        return Some(4 + i as u8);
    }
    let digits = [Num1, Num2, Num3, Num4, Num5, Num6, Num7, Num8, Num9, Num0];
    if let Some(i) = digits.iter().position(|&d| d == k) {
        return Some(30 + i as u8);
    }
    let f = [F1, F2, F3, F4, F5, F6, F7, F8, F9, F10, F11, F12];
    if let Some(i) = f.iter().position(|&d| d == k) {
        return Some(58 + i as u8);
    }
    let f2 = [F13, F14, F15, F16, F17, F18, F19, F20, F21, F22, F23, F24];
    if let Some(i) = f2.iter().position(|&d| d == k) {
        return Some(104 + i as u8);
    }
    Some(match k {
        Enter => 40,
        Escape => 41,
        Backspace => 42,
        Tab => 43,
        Space => 44,
        Minus => 45,
        Equals => 46,
        OpenBracket => 47,
        CloseBracket => 48,
        Backslash => 49,
        Semicolon => 51,
        Quote => 52,
        Backtick => 53,
        Comma => 54,
        Period => 55,
        Slash => 56,
        Insert => 73,
        Home => 74,
        PageUp => 75,
        Delete => 76,
        End => 77,
        PageDown => 78,
        ArrowRight => 79,
        ArrowLeft => 80,
        ArrowDown => 81,
        ArrowUp => 82,
        _ => return None,
    })
}

fn mouse_code(b: PointerButton) -> Option<u8> {
    match b {
        PointerButton::Primary => MouseButton::Left.macro_code(),
        PointerButton::Secondary => MouseButton::Right.macro_code(),
        PointerButton::Middle => MouseButton::Middle.macro_code(),
        PointerButton::Extra1 => MouseButton::Back.macro_code(),
        PointerButton::Extra2 => MouseButton::Forward.macro_code(),
    }
}

fn input_name(i: &MacroInput) -> String {
    match *i {
        MacroInput::Key(k) => labels::key_name(k),
        MacroInput::Mouse(c) => MouseButton::from_macro_code(c).map(labels::mouse_name).unwrap_or("Mouse").to_string(),
    }
}

impl Recorder {
    fn push(&mut self, m: &mut Macro, input: MacroInput, down: bool, fixed: Option<u16>) {
        let now = Instant::now();
        // Each event carries the delay *after* it; fill in the previous one.
        if let (Some(last), Some(MacroEvent::Press { delay, .. })) = (self.last, m.events.last_mut()) {
            *delay = fixed.unwrap_or_else(|| now.duration_since(last).as_millis().min(65535) as u16);
        }
        let mut next = m.clone();
        next.events.push(MacroEvent::Press { input: input.clone(), down, delay: fixed.unwrap_or(50) });
        if next.fits() {
            *m = next;
        }
        self.last = Some(now);
    }
}

impl MacrosPage {
    pub fn ui(&mut self, ui: &mut egui::Ui, cx: &mut Ctx) {
        super::header(ui, "03", "Macros", "Record key and mouse sequences, then assign them on the Keys page.");
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(300.0);
                self.library(ui, cx);
            });
            ui.add_space(16.0);
            ui.vertical(|ui| self.editor(ui, cx));
        });
    }

    fn library(&mut self, ui: &mut egui::Ui, cx: &mut Ctx) {
        theme::card(ui, "Library", |ui| {
            if cx.store.macros.is_empty() {
                ui.label(RichText::new("No macros yet.").color(theme::TEXT_FAINT));
            }
            for (i, m) in cx.store.macros.iter().enumerate() {
                let sel = self.current == Some(i);
                let r = ui.add(egui::Button::new(&m.name).selected(sel).min_size(egui::vec2(ui.available_width(), 30.0)));
                if r.clicked() {
                    self.current = Some(i);
                    self.draft = Some(m.clone());
                    self.recording = None;
                }
            }
            ui.add_space(8.0);
            if theme::ghost_button(ui, "New macro").clicked() {
                let n = cx.store.macros.len() + 1;
                cx.store.macros.push(NamedMacro { name: format!("Macro {n}"), data: Macro { repeat: 1, events: vec![] } });
                cx.store.save();
                self.current = Some(cx.store.macros.len() - 1);
                self.draft = cx.store.macros.last().cloned();
            }
        });
        ui.add_space(12.0);
        theme::card(ui, "Stored on keyboard", |ui| {
            if cx.snap.macros.is_empty() {
                ui.label(RichText::new("No keys use a macro.").color(theme::TEXT_FAINT));
            }
            let mut import = None;
            for (idx, m) in &cx.snap.macros {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Slot {}", idx + 1)).font(theme::mono(11.0)));
                    ui.label(RichText::new(format!("{} events", m.events.len())).color(theme::TEXT_FAINT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Copy to library").clicked() {
                            import = Some((*idx, m.clone()));
                        }
                    });
                });
            }
            if let Some((idx, m)) = import {
                cx.store.macros.push(NamedMacro { name: format!("Keyboard slot {}", idx + 1), data: m });
                cx.store.save();
                cx.note("Macro copied to the library");
            }
        });
    }

    fn editor(&mut self, ui: &mut egui::Ui, cx: &mut Ctx) {
        let Some(draft) = self.draft.as_mut() else {
            ui.label(RichText::new("Select or create a macro.").color(theme::TEXT_DIM));
            return;
        };
        theme::card(ui, "", |ui| {
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut draft.name)
                        .font(theme::heading(18.0))
                        .desired_width(260.0)
                        .char_limit(20)
                        .frame(egui::Frame::NONE),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let used = draft.data.byte_len();
                    let c = if used > Macro::MAX_EVENT_BYTES { theme::RED } else { theme::TEXT_FAINT };
                    ui.label(RichText::new(format!("{used} / {} BYTES", Macro::MAX_EVENT_BYTES)).font(theme::mono(10.5)).color(c));
                });
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.label(theme::label("Repeat"));
                ui.add(egui::DragValue::new(&mut draft.data.repeat).range(1..=65535));
                ui.add_space(16.0);
                ui.label(theme::label("Delays"));
                let mut fixed = self.fixed_delay.is_some();
                if theme::chip(ui, !fixed, "As recorded").clicked() {
                    fixed = false;
                }
                if theme::chip(ui, fixed, "Fixed").clicked() {
                    fixed = true;
                }
                if fixed {
                    let v = self.fixed_delay.get_or_insert(1);
                    ui.add(egui::DragValue::new(v).range(0..=65535).suffix(" ms"));
                } else {
                    self.fixed_delay = None;
                }
            });
            ui.add_space(12.0);

            // Recording surface: captures keys and mouse buttons while active.
            let rec = self.recording.is_some() || self.capture;
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 64.0), Sense::click());
            let stroke = if rec { Stroke::new(1.5, theme::RED) } else { Stroke::new(1.0, theme::BORDER_STRONG) };
            ui.painter().rect(rect, 0.0, theme::BG, stroke, egui::StrokeKind::Inside);
            let msg = if self.capture {
                "Press a key or click here to insert it"
            } else if rec {
                "Recording · press keys or click here · Esc or the button below stops"
            } else {
                "Press Record, then type the sequence"
            };
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, theme::spaced(msg), theme::mono(10.5), if rec {
                theme::RED
            } else {
                theme::TEXT_FAINT
            });
            if self.capture {
                resp.request_focus();
                let delay = self.fixed_delay.unwrap_or(50);
                let got = ui.input(|i| {
                    i.events.iter().find_map(|e| match e {
                        egui::Event::Key { key, physical_key, pressed: true, .. } => {
                            hid_of(physical_key.unwrap_or(*key)).map(MacroInput::Key)
                        }
                        egui::Event::PointerButton { pos, button, pressed: true, .. } if rect.contains(*pos) => {
                            mouse_code(*button).map(MacroInput::Mouse)
                        }
                        _ => None,
                    })
                });
                if let Some(input) = got {
                    draft.data.events.push(MacroEvent::Press { input: input.clone(), down: true, delay });
                    draft.data.events.push(MacroEvent::Press { input, down: false, delay });
                    self.capture = false;
                }
            }
            if let Some(r) = self.recording.as_mut() {
                resp.request_focus();
                let events = ui.input(|i| i.events.clone());
                let mods = ui.input(|i| i.modifiers);
                let pairs = [
                    (r.mods.shift, mods.shift, 225u8),
                    (r.mods.ctrl, mods.ctrl, 224),
                    (r.mods.alt, mods.alt, 226),
                    (r.mods.mac_cmd || r.mods.command && !r.mods.ctrl, mods.mac_cmd || mods.command && !mods.ctrl, 227),
                ];
                for (was, is, hid) in pairs {
                    if was != is {
                        r.push(&mut draft.data, MacroInput::Key(hid), is, self.fixed_delay);
                    }
                }
                r.mods = mods;
                let mut stop = false;
                for e in events {
                    match e {
                        egui::Event::Key { key: Key::Escape, pressed: true, .. } => stop = true,
                        egui::Event::Key { key, physical_key, pressed, repeat: false, .. } => {
                            if let Some(h) = hid_of(physical_key.unwrap_or(key)) {
                                r.push(&mut draft.data, MacroInput::Key(h), pressed, self.fixed_delay);
                            }
                        }
                        egui::Event::PointerButton { pos, button, pressed, .. } if rect.contains(pos) => {
                            if let Some(c) = mouse_code(button) {
                                r.push(&mut draft.data, MacroInput::Mouse(c), pressed, self.fixed_delay);
                            }
                        }
                        _ => {}
                    }
                }
                if stop {
                    self.recording = None;
                }
                ui.ctx().request_repaint();
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if self.recording.is_none() {
                    if theme::primary_button(ui, "● Record").clicked() {
                        self.recording = Some(Recorder { last: None, mods: ui.input(|i| i.modifiers) });
                    }
                } else if theme::danger_button(ui, "■ Stop").clicked() {
                    self.recording = None;
                }
                if theme::ghost_button(ui, "Clear").clicked() {
                    draft.data.events.clear();
                }
                if theme::ghost_button(ui, "+ Key").on_hover_text("Insert a key or mouse click").clicked() {
                    self.recording = None;
                    self.capture = true;
                }
                if theme::ghost_button(ui, "+ Move").on_hover_text("Insert a mouse movement").clicked() {
                    let delay = self.fixed_delay.unwrap_or(50);
                    draft.data.events.push(MacroEvent::Move { dx: 0, dy: 0, delay });
                }
            });
            ui.add_space(12.0);

            let mut remove = None;
            egui::ScrollArea::vertical().max_height(320.0).id_salt("events").show(ui, |ui| {
                for (n, e) in draft.data.events.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{:02}", n + 1)).font(theme::mono(10.5)).color(theme::TEXT_FAINT));
                        match e {
                            MacroEvent::Press { input, down, delay } => {
                                let (sym, c) = if *down { ("↓", theme::GREEN) } else { ("↑", theme::TEXT_DIM) };
                                ui.label(RichText::new(sym).color(c));
                                ui.add_sized([150.0, 20.0], egui::Label::new(input_name(input)));
                                ui.add(egui::DragValue::new(delay).range(1..=65535).suffix(" ms"));
                            }
                            MacroEvent::Move { dx, dy, delay } => {
                                ui.label("Move");
                                ui.add(egui::DragValue::new(dx).range(-127..=127).prefix("x "));
                                ui.add(egui::DragValue::new(dy).range(-127..=127).prefix("y "));
                                ui.add(egui::DragValue::new(delay).range(1..=65535).suffix(" ms"));
                            }
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✕").clicked() {
                                remove = Some(n);
                            }
                        });
                    });
                }
            });
            if let Some(n) = remove {
                draft.data.events.remove(n);
            }
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                let fits = draft.data.fits();
                let open: Vec<String> = draft.data.unreleased().iter().map(input_name).collect();
                let name_ok = !draft.name.trim().is_empty();
                let ok = fits && open.is_empty() && name_ok;
                if ui.add_enabled_ui(ok, |ui| theme::primary_button(ui, "Save")).inner.clicked() && ok {
                    if let Some(i) = self.current {
                        let old = cx.store.macros[i].data.clone();
                        cx.store.macros[i] = draft.clone();
                        cx.store.save();
                        // Keep keyboard copies of this macro in sync.
                        for (idx, m) in cx.snap.macros.iter_mut() {
                            if *m == old {
                                *m = draft.data.clone();
                                let _ = cx.tx.send(Command::SetMacro { index: *idx, data: draft.data.clone() });
                            }
                        }
                        cx.note("Macro saved");
                    }
                }
                if !fits {
                    ui.label(RichText::new("Too long for the keyboard").color(theme::RED));
                } else if !open.is_empty() {
                    ui.label(RichText::new(format!("{} has no release event", open.join(", "))).color(theme::RED));
                } else if !name_ok {
                    ui.label(RichText::new("Name the macro").color(theme::RED));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::danger_button(ui, "Delete").clicked() {
                        self.confirm_delete = true;
                    }
                });
            });
        });
        if self.confirm_delete {
            let mut open = true;
            super::keys::modal(ui.ctx(), "Delete macro", &mut open, |ui| {
                ui.label("Remove this macro from the library? Keys already using it keep their copy.");
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "Delete").clicked() {
                        if let Some(i) = self.current.take() {
                            cx.store.macros.remove(i);
                            cx.store.save();
                        }
                        self.draft = None;
                        self.confirm_delete = false;
                    }
                    if theme::ghost_button(ui, "Cancel").clicked() {
                        self.confirm_delete = false;
                    }
                });
            });
            if !open {
                self.confirm_delete = false;
            }
        }
    }
}
