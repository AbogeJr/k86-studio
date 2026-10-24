//! Key remapping for the three profiles, base and Fn layers.

use eframe::egui::{self, RichText};

use super::keyboard::{key_slot, KeyView};
use super::labels;
use super::Ctx;
use crate::defaults::{DEFAULT_FN_MATRIX, DEFAULT_MATRIX};
use crate::device::Command;
use crate::keymap::{self, KeyAction, MacroMode, MouseButton};
use crate::layout::{self, KEYS};
use crate::proto;
use crate::theme;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum Tab {
    #[default]
    Keys,
    Functions,
    Macro,
    Disable,
}

#[derive(Default)]
pub struct KeysPage {
    fn_layer: bool,
    selected: Option<usize>,
    tab: Tab,
    combo: Vec<u8>,
    macro_pick: Option<usize>,
    macro_mode: Option<MacroMode>,
    confirm_restore: bool,
}

/// Fn and right-Fn cannot be reassigned (the firmware needs them).
fn locked(i: usize) -> bool {
    let hid = KEYS[i].hid;
    !KEYS[i].remappable || hid == 0x0A01_0000 || hid == 0x0A01_0100
}

impl KeysPage {
    fn matrix<'a>(&self, cx: &'a Ctx) -> &'a [u8] {
        if self.fn_layer { &cx.snap.fn_keys } else { &cx.snap.keys[cx.snap.profile as usize] }
    }

    fn action_at(&self, cx: &Ctx, i: usize) -> Option<(KeyAction, bool)> {
        let slot = key_slot(i)?;
        let raw = keymap::slot(self.matrix(cx), slot);
        let default = if self.fn_layer { keymap::default_fn_slot(slot) } else { keymap::default_slot(slot) };
        let a = if self.fn_layer { KeyAction::decode_fn(raw) } else { KeyAction::decode(raw) };
        Some((a, raw != default))
    }

    fn assign(&mut self, cx: &mut Ctx, action: KeyAction) {
        let Some(slot) = self.selected.and_then(key_slot) else { return };
        cx.send(Command::SetKey {
            layer: if self.fn_layer { 0 } else { cx.snap.profile },
            slot: slot as u8,
            value: action.encode(),
            fn_layer: self.fn_layer,
        });
        // Optimistic update; the worker sends the read-back matrix afterwards.
        let m = if self.fn_layer { &mut cx.snap.fn_keys } else { &mut cx.snap.keys[cx.snap.profile as usize] };
        keymap::set_slot(m, slot, action.encode());
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, cx: &mut Ctx) {
        super::header(ui, "01", "Keys", "Remap any key, per profile, on the base layer or under Fn.");

        ui.horizontal(|ui| {
            ui.label(theme::label("Profile"));
            for p in 0..proto::LAYERS {
                if theme::chip(ui, cx.snap.profile == p, &format!("{}", p + 1)).clicked() && !cx.busy {
                    cx.send(Command::SetProfile(p));
                    cx.snap.profile = p;
                }
            }
            ui.add_space(18.0);
            ui.label(theme::label("Layer"));
            if theme::chip(ui, !self.fn_layer, "Base").clicked() {
                self.fn_layer = false;
            }
            if theme::chip(ui, self.fn_layer, "Fn").clicked() {
                self.fn_layer = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if theme::ghost_button(ui, "Restore layer").clicked() {
                    self.confirm_restore = true;
                }
            });
        });
        ui.add_space(14.0);

        let captions = |i: usize| -> Option<String> {
            let (a, changed) = self.action_at(cx, i)?;
            changed.then(|| labels::action(&a))
        };
        let selected: Vec<usize> = self.selected.into_iter().collect();
        let fn_layer = self.fn_layer;
        let fn_keys = cx.snap.fn_keys.clone();
        // On the Fn layer, firmware-owned system functions are read-only.
        let enabled = move |i: usize| {
            !locked(i)
                && !(fn_layer
                    && key_slot(i).is_some_and(|s| matches!(KeyAction::decode_fn(keymap::slot(&fn_keys, s)), KeyAction::Raw(_))))
        };
        let fill = |_: usize| None;
        let resp = KeyView { caption: &captions, fill: &fill, selected: &selected, enabled: &enabled }.show(ui);
        if let Some(i) = resp.clicked {
            self.selected = if self.selected == Some(i) { None } else { Some(i) };
            self.combo.clear();
            if let Some((a, _)) = self.action_at(cx, i) {
                self.combo = a.pressed();
            }
        }

        ui.add_space(16.0);
        match self.selected {
            None => {
                ui.label(RichText::new("Select a key to change what it does.").color(theme::TEXT_DIM));
            }
            Some(i) => self.editor(ui, cx, i),
        }

        if self.confirm_restore {
            let mut open = true;
            modal(ui.ctx(), "Restore layer", &mut open, |ui| {
                if self.fn_layer {
                    ui.label("Reset every Fn-layer function to its factory setting?");
                } else {
                    ui.label(format!("Reset every key in profile {} to its factory function?", cx.snap.profile + 1));
                }
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "Restore").clicked() {
                        let matrix = if self.fn_layer { DEFAULT_FN_MATRIX.to_vec() } else { DEFAULT_MATRIX.to_vec() };
                        let layer = if self.fn_layer { 0 } else { cx.snap.profile };
                        cx.send(Command::SetMatrix { layer, matrix, fn_layer: self.fn_layer });
                        self.confirm_restore = false;
                    }
                    if theme::ghost_button(ui, "Cancel").clicked() {
                        self.confirm_restore = false;
                    }
                });
            });
            if !open {
                self.confirm_restore = false;
            }
        }
    }

    fn editor(&mut self, ui: &mut egui::Ui, cx: &mut Ctx, i: usize) {
        let Some((current, changed)) = self.action_at(cx, i) else { return };
        theme::card(ui, "", |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(KEYS[i].label.replace('\n', " ")).font(theme::heading(18.0)));
                ui.label(RichText::new("→").color(theme::TEXT_FAINT));
                ui.label(RichText::new(labels::action(&current)).font(theme::body_bold(14.0)).color(if changed {
                    theme::ACCENT
                } else {
                    theme::TEXT
                }));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if changed && theme::ghost_button(ui, "Reset key").clicked() {
                        let slot = key_slot(i).unwrap_or(0);
                        let d = if self.fn_layer { keymap::default_fn_slot(slot) } else { keymap::default_slot(slot) };
                        self.assign(cx, KeyAction::Raw(d));
                        self.combo.clear();
                    }
                });
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                for (t, name) in [
                    (Tab::Keys, "Combo keys"),
                    (Tab::Functions, "Functions"),
                    (Tab::Macro, "Macro"),
                    (Tab::Disable, if self.fn_layer { "Clear" } else { "Disable" }),
                ] {
                    if theme::chip(ui, self.tab == t, name).clicked() {
                        self.tab = t;
                    }
                }
            });
            ui.add_space(12.0);
            match self.tab {
                Tab::Keys => self.combo_tab(ui, cx),
                Tab::Functions => self.function_tab(ui, cx, &current),
                Tab::Macro => self.macro_tab(ui, cx),
                Tab::Disable => {
                    let what = if self.fn_layer { "Remove the Fn function from this key." } else { "The key will do nothing." };
                    ui.label(RichText::new(what).color(theme::TEXT_DIM));
                    ui.add_space(8.0);
                    if theme::primary_button(ui, if self.fn_layer { "Clear" } else { "Disable key" }).clicked() {
                        self.assign(cx, KeyAction::Disabled);
                    }
                }
            }
        });
    }

    fn combo_tab(&mut self, ui: &mut egui::Ui, cx: &mut Ctx) {
        ui.horizontal(|ui| {
            ui.label(theme::label("Keys"));
            if self.combo.is_empty() {
                ui.label(RichText::new("Pick up to three keys below").color(theme::TEXT_FAINT));
            }
            let mut remove = None;
            for (n, &h) in self.combo.iter().enumerate() {
                if n > 0 {
                    ui.label(RichText::new("+").color(theme::TEXT_FAINT));
                }
                if ui.add(egui::Button::new(labels::key_name(h)).selected(true)).on_hover_text("Remove").clicked() {
                    remove = Some(n);
                }
            }
            if let Some(n) = remove {
                self.combo.remove(n);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ok = !self.combo.is_empty();
                if ui.add_enabled_ui(ok, |ui| theme::primary_button(ui, "Apply")).inner.clicked() && ok {
                    let a = KeyAction::keys(&self.combo);
                    self.assign(cx, a);
                }
                if theme::ghost_button(ui, "Clear").clicked() {
                    self.combo.clear();
                }
            });
        });
        ui.add_space(10.0);
        for (title, keys) in [("Basic characters", layout::BASIC_KEYS), ("Extended characters", layout::EXTENDED_KEYS)] {
            ui.label(theme::label(title));
            ui.add_space(4.0);
            let chosen = self.combo.clone();
            key_grid(ui, keys.iter().map(|(n, h)| (n.replace('\n', " "), *h)), |h| {
                if let Some(p) = self.combo.iter().position(|&x| x == h) {
                    self.combo.remove(p);
                } else if self.combo.len() < 3 {
                    self.combo.push(h);
                }
            }, &chosen);
            ui.add_space(10.0);
        }
    }

    fn function_tab(&mut self, ui: &mut egui::Ui, cx: &mut Ctx, current: &KeyAction) {
        let cur = current.encode();
        for (title, items) in [("Functions", layout::FUNCTIONS), ("Mouse", layout::MOUSE_FUNCTIONS)] {
            ui.label(theme::label(title));
            ui.add_space(4.0);
            let mut pick = None;
            ui.horizontal_wrapped(|ui| {
                for (name, bytes) in items {
                    if ui.add(egui::Button::new(*name).selected(*bytes == cur).min_size(egui::vec2(110.0, 30.0))).clicked() {
                        pick = Some(*bytes);
                    }
                }
            });
            if let Some(b) = pick {
                let a = MouseButton::from_bytes(b).map(KeyAction::Mouse).unwrap_or_else(|| KeyAction::decode(b));
                self.assign(cx, a);
            }
            ui.add_space(10.0);
        }
    }

    fn macro_tab(&mut self, ui: &mut egui::Ui, cx: &mut Ctx) {
        if cx.store.macros.is_empty() {
            ui.label(RichText::new("No macros yet. Record one on the Macros page.").color(theme::TEXT_DIM));
            return;
        }
        ui.horizontal(|ui| {
            ui.label(theme::label("Macro"));
            let names: Vec<String> = cx.store.macros.iter().map(|m| m.name.clone()).collect();
            let sel = self.macro_pick.and_then(|i| names.get(i)).cloned().unwrap_or_else(|| "Choose…".into());
            egui::ComboBox::from_id_salt("macro-pick").selected_text(sel).show_ui(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    ui.selectable_value(&mut self.macro_pick, Some(i), n);
                }
            });
            ui.add_space(12.0);
            ui.label(theme::label("Playback"));
            let mode = self.macro_mode.get_or_insert(MacroMode::RepeatTimes);
            egui::ComboBox::from_id_salt("macro-mode").selected_text(labels::macro_mode_name(*mode)).show_ui(ui, |ui| {
                for m in MacroMode::ALL {
                    ui.selectable_value(mode, m, labels::macro_mode_name(m));
                }
            });
        });
        if let Some(m) = self.macro_pick.and_then(|i| cx.store.macros.get(i)) {
            ui.add_space(6.0);
            ui.label(
                RichText::new(format!("{} events · {} of 254 bytes", m.data.events.len(), m.data.byte_len()))
                    .font(theme::mono(10.5))
                    .color(theme::TEXT_FAINT),
            );
        }
        ui.add_space(10.0);
        let ready = self.macro_pick.is_some();
        if ui.add_enabled_ui(ready, |ui| theme::primary_button(ui, "Assign macro")).inner.clicked() && ready {
            let Some(slot) = self.selected.and_then(key_slot) else { return };
            let m = cx.store.macros[self.macro_pick.unwrap()].data.clone();
            let index = macro_index(cx.snap, self.fn_layer, slot);
            let mode = self.macro_mode.unwrap_or(MacroMode::RepeatTimes);
            cx.send(Command::SetMacro { index, data: m.clone() });
            cx.snap.macros.retain(|(i, _)| *i != index);
            cx.snap.macros.push((index, m));
            self.assign(cx, KeyAction::Macro { mode, index });
        }
    }
}

/// Macro slot for a key: reuse the key's existing slot, else the lowest free one.
fn macro_index(snap: &crate::device::Snapshot, fn_layer: bool, slot: usize) -> u8 {
    let m = if fn_layer { &snap.fn_keys } else { &snap.keys[snap.profile as usize] };
    let raw = keymap::slot(m, slot);
    if raw[0] == 9 {
        return raw[2];
    }
    let used: Vec<u8> = snap
        .keys
        .iter()
        .chain(std::iter::once(&snap.fn_keys))
        .flat_map(|m| (0..proto::SLOTS).map(move |i| keymap::slot(m, i)))
        .filter(|b| b[0] == 9)
        .map(|b| b[2])
        .collect();
    (0..crate::device::MACRO_SLOTS).find(|i| !used.contains(i)).unwrap_or(0)
}

fn key_grid(ui: &mut egui::Ui, keys: impl Iterator<Item = (String, u8)>, mut on: impl FnMut(u8), chosen: &[u8]) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
        for (name, hid) in keys {
            let b = egui::Button::new(RichText::new(name).font(theme::mono(11.0)))
                .selected(chosen.contains(&hid))
                .min_size(egui::vec2(44.0, 30.0));
            if ui.add(b).clicked() {
                on(hid);
            }
        }
    });
}

/// Centered dialog in the launcher's style.
pub fn modal(ctx: &egui::Context, title: &str, open: &mut bool, add: impl FnOnce(&mut egui::Ui)) {
    egui::Modal::new(egui::Id::new(("modal", title))).show(ctx, |ui| {
        ui.set_width(420.0);
        ui.label(RichText::new(title).font(theme::heading(18.0)));
        ui.add_space(10.0);
        add(ui);
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        *open = false;
    }
}
