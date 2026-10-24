pub mod firmware;
pub mod keyboard;
pub mod keys;
pub mod labels;
pub mod lighting;
pub mod macros;
pub mod screen;
pub mod settings;

use std::sync::mpsc::Sender;

use crate::device::{Command, Snapshot};
use crate::store::Store;

/// What a page gets to work with each frame.
pub struct Ctx<'a> {
    pub snap: &'a mut Snapshot,
    pub tx: &'a Sender<Command>,
    pub store: &'a mut Store,
    /// A long operation is running; pages should not start another.
    pub busy: bool,
    pub notes: &'a mut Vec<String>,
}

impl Ctx<'_> {
    pub fn send(&self, c: Command) {
        let _ = self.tx.send(c);
    }

    pub fn note(&mut self, s: impl Into<String>) {
        self.notes.push(s.into());
    }
}

/// Page header: ArtCraft-style numbered badge, title and a one-line description.
pub fn header(ui: &mut eframe::egui::Ui, n: &str, title: &str, sub: &str) {
    use crate::theme;
    ui.horizontal(|ui| {
        theme::badge(ui, n, theme::ACCENT.gamma_multiply(0.85));
        ui.add_space(4.0);
        ui.label(eframe::egui::RichText::new(title).font(theme::heading(22.0)).color(theme::TEXT));
    });
    ui.label(eframe::egui::RichText::new(sub).color(theme::TEXT_DIM));
    ui.add_space(14.0);
}
