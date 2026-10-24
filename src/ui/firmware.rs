//! Firmware versions, online update check and flashing.

use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

use eframe::egui::{self, RichText};

use super::Ctx;
use crate::device::Command;
use crate::firmware::{self, Package, Update};
use crate::theme;

enum Job {
    Check(Result<Update, String>),
    Download(Result<Package, String>),
    Progress(f32),
}

#[derive(Default)]
pub struct FirmwarePage {
    rx: Option<Receiver<Job>>,
    checking: bool,
    downloading: Option<f32>,
    update: Option<Update>,
    package: Option<Package>,
    status: Option<(String, bool)>,
    confirm_since: Option<Instant>,
}

fn version(v: u16) -> String {
    format!("V{}", firmware::display_version(v).to_uppercase())
}

impl FirmwarePage {
    fn poll(&mut self, cx: &mut Ctx) {
        let Some(rx) = &self.rx else { return };
        while let Ok(j) = rx.try_recv() {
            match j {
                Job::Check(Ok(u)) => {
                    self.checking = false;
                    self.status = Some(if u.any() {
                        ("A firmware update is available.".into(), false)
                    } else {
                        ("Your firmware is up to date.".into(), false)
                    });
                    self.update = Some(u);
                }
                Job::Check(Err(e)) => {
                    self.checking = false;
                    self.status = Some((e, true));
                }
                Job::Progress(p) => self.downloading = Some(p),
                Job::Download(Ok(p)) => {
                    self.downloading = None;
                    self.package = Some(p);
                    self.confirm_since = Some(Instant::now());
                }
                Job::Download(Err(e)) => {
                    self.downloading = None;
                    cx.note(format!("Download failed: {e}"));
                }
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, cx: &mut Ctx) {
        self.poll(cx);
        super::header(ui, "06", "Firmware", "Check for and install official firmware for your K86.");

        theme::card(ui, "Installed", |ui| {
            egui::Grid::new("fw").num_columns(2).spacing([32.0, 8.0]).show(ui, |ui| {
                ui.label(theme::label("Device ID"));
                ui.label(RichText::new(crate::proto::DEVICE_ID.to_string()).font(theme::mono(12.0)));
                ui.end_row();
                ui.label(theme::label("Keyboard"));
                ui.label(RichText::new(version(cx.snap.firmware)).font(theme::mono(12.0)));
                ui.end_row();
                if let Some((oled, flash)) = cx.snap.screen_firmware {
                    ui.label(theme::label("Screen"));
                    ui.label(RichText::new(format!("{} · flash {}", version(oled), version(flash))).font(theme::mono(12.0)));
                    ui.end_row();
                }
            });
        });
        ui.add_space(12.0);

        theme::card(ui, "Update", |ui| {
            ui.horizontal(|ui| {
                let busy = self.checking || self.downloading.is_some();
                if ui.add_enabled_ui(!busy && !cx.busy, |ui| theme::primary_button(ui, "Check for updates")).inner.clicked() {
                    let (tx, rx) = mpsc::channel();
                    let (fw, oled) = (cx.snap.firmware, cx.snap.screen_firmware.map(|v| v.0));
                    let ctx = ui.ctx().clone();
                    std::thread::spawn(move || {
                        let r = firmware::check(fw, oled).map_err(|e| e.to_string());
                        let _ = tx.send(Job::Check(r));
                        ctx.request_repaint();
                    });
                    self.rx = Some(rx);
                    self.checking = true;
                    self.status = None;
                    self.update = None;
                }
                if self.checking {
                    ui.add(egui::Spinner::new().color(theme::TEXT_FAINT));
                }
            });
            if let Some((s, err)) = &self.status {
                ui.add_space(8.0);
                ui.label(RichText::new(s).color(if *err { theme::RED } else { theme::TEXT_DIM }));
            }
            if let Some(u) = self.update.clone().filter(Update::any) {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    theme::pill(ui, "Update available", theme::AMBER);
                    ui.label(RichText::new(&u.info.version_str).font(theme::mono(12.0)));
                });
                if let Some(v) = u.main {
                    ui.label(format!("Keyboard {} → {}", version(cx.snap.firmware), version(v)));
                }
                if let Some(v) = u.oled {
                    ui.label(format!("Screen → {}", version(v)));
                }
                ui.add_space(10.0);
                if let Some(p) = self.downloading {
                    crate::app::progress_bar(ui, p);
                } else if ui.add_enabled_ui(!cx.busy, |ui| theme::primary_button(ui, "Download and install")).inner.clicked() {
                    let (tx, rx) = mpsc::channel();
                    let ctx = ui.ctx().clone();
                    std::thread::spawn(move || {
                        let tx2 = tx.clone();
                        let c2 = ctx.clone();
                        let r = firmware::download(&u, move |p| {
                            let _ = tx2.send(Job::Progress(p));
                            c2.request_repaint();
                        })
                        .map_err(|e| e.to_string());
                        let _ = tx.send(Job::Download(r));
                        ctx.request_repaint();
                    });
                    self.rx = Some(rx);
                    self.downloading = Some(0.0);
                }
            }
            ui.add_space(10.0);
            ui.label(
                RichText::new("Updates use the USB cable only. Remove the 2.4G receiver before updating.")
                    .small()
                    .color(theme::TEXT_FAINT),
            );
        });

        if let Some(since) = self.confirm_since {
            let left = 3u64.saturating_sub(since.elapsed().as_secs());
            let mut open = true;
            super::keys::modal(ui.ctx(), "Install firmware", &mut open, |ui| {
                ui.label(RichText::new("Do not unplug the keyboard or close this app until the update finishes.").color(theme::AMBER));
                ui.add_space(6.0);
                ui.label("The keyboard restarts into update mode and its keys stop working until it is done.");
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    let label = if left > 0 { format!("Install ({left})") } else { "Install".into() };
                    if ui.add_enabled_ui(left == 0, |ui| theme::primary_button(ui, &label)).inner.clicked() && left == 0 {
                        if let Some(p) = self.package.take() {
                            cx.send(Command::Firmware(p));
                        }
                        self.confirm_since = None;
                        self.update = None;
                    }
                    if theme::ghost_button(ui, "Cancel").clicked() {
                        self.confirm_since = None;
                    }
                });
            });
            if left > 0 {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
            }
            if !open {
                self.confirm_since = None;
            }
        }
    }
}
