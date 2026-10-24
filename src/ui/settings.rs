//! Debounce, sleep timers, app behaviour and factory reset.

use eframe::egui::{self, RichText};

use super::Ctx;
use crate::device::{Command, Snapshot};
use crate::proto::SleepTimes;
use crate::theme;

#[derive(Default)]
pub struct SettingsPage {
    debounce: u8,
    /// Minutes: 2.4G light-off, 2.4G deep, BT light-off, BT deep.
    sleep: [u16; 4],
    dirty_at: Option<std::time::Instant>,
    confirm_reset: bool,
    autostart: Option<bool>,
}

fn minutes(secs: u16) -> u16 {
    (secs as f32 / 60.0).round() as u16
}

impl SettingsPage {
    pub fn sync(&mut self, s: &Snapshot) {
        self.debounce = s.debounce.clamp(1, 10);
        self.sleep = [
            minutes(s.sleep.rf24).clamp(1, 60),
            minutes(s.sleep.deep_rf24).clamp(10, 60),
            minutes(s.sleep.bt).clamp(1, 60),
            minutes(s.sleep.deep_bt).clamp(10, 60),
        ];
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, cx: &mut Ctx) {
        super::header(ui, "05", "Settings", "Keyboard behaviour stored on the K86, and how this app runs.");
        let mut changed = false;
        ui.columns(2, |cols| {
            theme::card(&mut cols[0], "Key debounce", |ui| {
                ui.label(RichText::new("Higher values ignore chatter from worn switches but add latency.").color(theme::TEXT_DIM));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("1").font(theme::mono(11.0)).color(theme::TEXT_FAINT));
                    changed |= ui.add(egui::Slider::new(&mut self.debounce, 1..=10)).changed();
                    ui.label(RichText::new("10").font(theme::mono(11.0)).color(theme::TEXT_FAINT));
                });
            });
            theme::card(&mut cols[1], "Sleep time", |ui| {
                let rows = [
                    ("2.4G · turn off backlight", 1..=60),
                    ("2.4G · deep sleep", 10..=60),
                    ("Bluetooth · turn off backlight", 1..=60),
                    ("Bluetooth · deep sleep", 10..=60),
                ];
                egui::Grid::new("sleep").num_columns(2).spacing([24.0, 10.0]).show(ui, |ui| {
                    for (i, (name, range)) in rows.into_iter().enumerate() {
                        ui.label(name);
                        changed |= ui.add(egui::DragValue::new(&mut self.sleep[i]).range(range).suffix(" min")).changed();
                        ui.end_row();
                    }
                });
                ui.add_space(4.0);
                ui.label(RichText::new("Applies when the keyboard runs wirelessly.").small().color(theme::TEXT_FAINT));
            });
        });
        if changed {
            self.dirty_at = Some(std::time::Instant::now());
        }
        // Write once the user stops adjusting (each write takes ~0.5 s on the keyboard).
        if let Some(t) = self.dirty_at {
            if t.elapsed().as_millis() > 400 && !ui.input(|i| i.pointer.any_down()) && !cx.busy {
                self.dirty_at = None;
                if self.debounce != cx.snap.debounce {
                    cx.snap.debounce = self.debounce;
                    cx.send(Command::SetDebounce(self.debounce));
                }
                let t = SleepTimes {
                    rf24: self.sleep[0] * 60,
                    deep_rf24: self.sleep[1] * 60,
                    bt: self.sleep[2] * 60,
                    deep_bt: self.sleep[3] * 60,
                };
                if t != cx.snap.sleep {
                    cx.snap.sleep = t;
                    cx.send(Command::SetSleep(t));
                }
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            }
        }

        ui.add_space(12.0);
        theme::card(ui, "App", |ui| {
            let mut auto = *self.autostart.get_or_insert_with(crate::autostart::enabled);
            if ui.checkbox(&mut auto, "Start on boot (hidden in the tray)").changed() {
                match crate::autostart::set(auto) {
                    Ok(()) => self.autostart = Some(auto),
                    Err(e) => cx.note(format!("Could not change autostart: {e}")),
                }
            }
            let mut tray = cx.store.close_to_tray.unwrap_or(true);
            if ui.checkbox(&mut tray, "Minimize to tray when closed").changed() {
                cx.store.close_to_tray = Some(tray);
                cx.store.save();
            }
        });

        ui.add_space(12.0);
        theme::card(ui, "Factory reset", |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Restores every profile, key, macro assignment and lighting setting.").color(theme::TEXT_DIM));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if theme::danger_button(ui, "Restore factory settings").clicked() {
                        self.confirm_reset = true;
                    }
                });
            });
        });

        if self.confirm_reset {
            let mut open = true;
            super::keys::modal(ui.ctx(), "Restore factory settings", &mut open, |ui| {
                ui.label("Everything stored on the keyboard goes back to its factory state. Macros in this app's library are kept.");
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, "Restore").clicked() {
                        cx.send(Command::FactoryReset);
                        self.confirm_reset = false;
                    }
                    if theme::ghost_button(ui, "Cancel").clicked() {
                        self.confirm_reset = false;
                    }
                });
            });
            if !open {
                self.confirm_reset = false;
            }
        }
    }
}
