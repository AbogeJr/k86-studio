//! Window chrome, navigation and event routing.

use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, CentralPanel, Color32, CursorIcon, Frame, Layout, Margin, ResizeDirection, RichText,
    Panel, Sense, Stroke, Vec2, ViewportCommand,
};

use crate::device::{self, Command, Event, Snapshot};
use crate::store::Store;
use crate::theme;
use crate::tray::TrayEvent;
use crate::ui;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Keys,
    Lighting,
    Macros,
    Screen,
    Settings,
    Firmware,
}

const PAGES: [(Page, &str); 6] = [
    (Page::Keys, "Keys"),
    (Page::Lighting, "Lighting"),
    (Page::Macros, "Macros"),
    (Page::Screen, "Screen"),
    (Page::Settings, "Settings"),
    (Page::Firmware, "Firmware"),
];

enum Link {
    Searching,
    NoAccess(String),
    Connected,
}

struct Toast {
    text: String,
    error: bool,
    at: Instant,
}

pub struct App {
    dev: device::Handle,
    events: Receiver<TrayEvent>,
    has_tray: bool,
    link: Link,
    snap: Option<Snapshot>,
    busy: Option<(String, Option<f32>)>,
    toasts: Vec<Toast>,
    page: Page,
    store: Store,
    logo: Option<egui::TextureHandle>,
    hidden: bool,
    keys: ui::keys::KeysPage,
    lighting: ui::lighting::LightingPage,
    macros: ui::macros::MacrosPage,
    screen: ui::screen::ScreenPage,
    settings: ui::settings::SettingsPage,
    firmware: ui::firmware::FirmwarePage,
}

impl App {
    pub fn new(cc: &eframe::CreationContext, start_hidden: bool, tx: Sender<TrayEvent>, rx: Receiver<TrayEvent>) -> Self {
        theme::install(&cc.egui_ctx);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let ctx = cc.egui_ctx.clone();
        let dev = device::spawn(move || ctx.request_repaint());
        let ctx = cc.egui_ctx.clone();
        let has_tray = crate::tray::spawn(tx, move || ctx.request_repaint());
        let logo = image::load_from_memory(include_bytes!("../assets/brand/app.png")).ok().map(|img| {
            let img = img.resize(64, 64, image::imageops::FilterType::Lanczos3).to_rgba8();
            let ci = egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw());
            cc.egui_ctx.load_texture("logo", ci, egui::TextureOptions::LINEAR)
        });
        if start_hidden && !has_tray {
            // Nowhere to restore from: start visible.
            cc.egui_ctx.send_viewport_cmd(ViewportCommand::Visible(true));
        }
        Self {
            dev,
            events: rx,
            has_tray,
            link: Link::Searching,
            snap: None,
            busy: None,
            toasts: Vec::new(),
            page: std::env::args()
                .find_map(|a| a.strip_prefix("--page=").map(str::to_lowercase))
                .and_then(|p| PAGES.iter().find(|(_, n)| n.to_lowercase() == p).map(|(pg, _)| *pg))
                .unwrap_or(Page::Keys),
            store: Store::load(),
            logo,
            hidden: start_hidden,
            keys: Default::default(),
            lighting: Default::default(),
            macros: Default::default(),
            screen: Default::default(),
            settings: Default::default(),
            firmware: Default::default(),
        }
    }

    fn hide(&mut self, ctx: &egui::Context) {
        self.hidden = true;
        ctx.send_viewport_cmd(ViewportCommand::Visible(false));
    }

    fn toast(&mut self, text: impl Into<String>, error: bool) {
        self.toasts.push(Toast { text: text.into(), error, at: Instant::now() });
    }

    fn pump(&mut self, ctx: &egui::Context) {
        while let Ok(ev) = self.dev.rx.try_recv() {
            match ev {
                Event::Searching => {
                    self.link = Link::Searching;
                    self.snap = None;
                    self.busy = None;
                }
                Event::NoAccess(m) => self.link = Link::NoAccess(m),
                Event::Connected(s) => {
                    self.link = Link::Connected;
                    self.lighting.sync(&s);
                    self.settings.sync(&s);
                    self.snap = Some(*s);
                    self.busy = None;
                }
                Event::Disconnected => {
                    self.snap = None;
                    self.busy = None;
                    self.toast("Keyboard disconnected", true);
                }
                Event::Busy(m) => self.busy = Some((m, None)),
                Event::Progress(p) => {
                    if let Some(b) = &mut self.busy {
                        b.1 = Some(p);
                    } else {
                        self.busy = Some(("Working".into(), Some(p)));
                    }
                }
                Event::Done(m) => {
                    self.busy = None;
                    self.toast(m, false);
                }
                Event::Failed(m) => {
                    self.busy = None;
                    self.toast(m, true);
                }
                Event::Profile(p) => {
                    if let Some(s) = &mut self.snap {
                        s.profile = p;
                    }
                }
                Event::Light(l) => {
                    if let Some(s) = &mut self.snap {
                        s.light = l;
                        self.lighting.sync(s);
                    }
                }
                Event::Keys { layer, matrix, fn_layer } => {
                    if let Some(s) = &mut self.snap {
                        if fn_layer {
                            s.fn_keys = matrix;
                        } else {
                            s.keys[layer as usize] = matrix;
                        }
                    }
                }
                Event::UserPicture(c) => self.lighting.set_picture(c),
            }
        }
        {
            let evs: Vec<TrayEvent> = self.events.try_iter().collect();
            for e in evs {
                match e {
                    TrayEvent::Show => {
                        self.hidden = false;
                        ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                        ctx.send_viewport_cmd(ViewportCommand::Focus);
                    }
                    TrayEvent::Hide => self.hide(ctx),
                    TrayEvent::Quit => {
                        self.store.save();
                        std::process::exit(0);
                    }
                }
            }
        }
    }

    fn title_bar(&mut self, ui: &mut egui::Ui) {
        let bar = ui.max_rect();
        let drag = ui.interact(bar, ui.id().with("drag"), Sense::click_and_drag());
        if drag.drag_started_by(egui::PointerButton::Primary) {
            ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
        }
        if drag.double_clicked() {
            let max = ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false));
            ui.ctx().send_viewport_cmd(ViewportCommand::Maximized(!max));
        }
        ui.horizontal_centered(|ui| {
            ui.add_space(14.0);
            if let Some(t) = &self.logo {
                ui.add(egui::Image::new(t).fit_to_exact_size(Vec2::splat(22.0)));
            }
            ui.add_space(2.0);
            theme::two_tone(ui, "ATTACK", "SHARK", theme::ACCENT, 17.0);
            ui.add_space(10.0);
            divider(ui);
            ui.add_space(10.0);
            theme::pill(ui, "K86", theme::TEXT_DIM);
            ui.add_space(16.0);
            for (p, name) in PAGES {
                if theme::chip(ui, self.page == p, name).clicked() {
                    self.page = p;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let ctx = ui.ctx().clone();
                if window_button(ui, Glyph::Close, true).clicked() {
                    if self.has_tray && self.store.close_to_tray.unwrap_or(true) {
                        self.hide(&ctx);
                    } else {
                        self.store.save();
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                }
                if window_button(ui, Glyph::Max, false).clicked() {
                    let max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                    ctx.send_viewport_cmd(ViewportCommand::Maximized(!max));
                }
                if window_button(ui, Glyph::Min, false).clicked() {
                    // Like the original: "–" sends the window to the tray.
                    if self.has_tray {
                        self.hide(&ctx);
                    } else {
                        ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                    }
                }
                divider(ui);
                let can = self.snap.is_some() && self.busy.is_none();
                let r = window_button(ui, Glyph::Reload, false).on_hover_text("Read settings from the keyboard again");
                if r.clicked() && can {
                    let _ = self.dev.tx.send(Command::Reload);
                }
            });
        });
    }

    fn footer(&self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(14.0);
            let (dot, text) = match (&self.link, &self.snap) {
                (Link::Connected, Some(s)) => (
                    theme::GREEN,
                    format!(
                        "Connected · K86 · USB · firmware V{:X} · {}",
                        s.firmware,
                        s.node
                    ),
                ),
                (Link::NoAccess(_), _) => (theme::AMBER, "Keyboard found, but access is denied".into()),
                _ => (theme::TEXT_FAINT, "Waiting for keyboard".into()),
            };
            let (r, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
            ui.painter().rect_filled(r, 0.0, dot);
            ui.label(RichText::new(theme::spaced(&text)).font(theme::mono(10.0)).color(theme::TEXT_FAINT));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(14.0);
                ui.label(
                    RichText::new(theme::spaced(concat!("K86 Studio ", env!("CARGO_PKG_VERSION"))))
                        .font(theme::mono(10.0))
                        .color(theme::TEXT_FAINT),
                );
                if let Some(s) = &self.snap {
                    divider(ui);
                    ui.label(
                        RichText::new(format!("PROFILE {}", s.profile + 1)).font(theme::mono_bold(10.0)).color(theme::TEXT_DIM),
                    );
                }
            });
        });
    }

    fn empty_state(&self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.28);
            if let Some(t) = &self.logo {
                ui.add(egui::Image::new(t).fit_to_exact_size(Vec2::splat(56.0)).tint(Color32::from_white_alpha(200)));
            }
            ui.add_space(14.0);
            match &self.link {
                Link::NoAccess(m) => {
                    ui.label(RichText::new("No permission to talk to the keyboard").font(theme::heading(20.0)));
                    ui.add_space(6.0);
                    ui.label(RichText::new(m).font(theme::mono(11.0)).color(theme::TEXT_FAINT));
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("Install the udev rule once, then unplug and replug the keyboard:")
                            .color(theme::TEXT_DIM),
                    );
                    ui.add_space(6.0);
                    let cmd = "sudo cp 70-attackshark.rules /etc/udev/rules.d/ && sudo udevadm control --reload";
                    ui.label(RichText::new(cmd).font(theme::mono(11.5)).color(theme::TEXT));
                }
                _ => {
                    ui.label(RichText::new("Connect your K86").font(theme::heading(22.0)));
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Plug the keyboard in with its USB cable. It will appear here automatically.")
                            .color(theme::TEXT_DIM),
                    );
                    ui.add_space(18.0);
                    ui.add(egui::Spinner::new().size(18.0).color(theme::TEXT_FAINT));
                }
            }
        });
    }

    fn toasts(&mut self, ctx: &egui::Context) {
        self.toasts.retain(|t| t.at.elapsed() < Duration::from_secs(4));
        if self.toasts.is_empty() {
            return;
        }
        egui::Area::new("toasts".into())
            .anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-16.0, -44.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                for t in &self.toasts {
                    let c = if t.error { theme::RED } else { theme::GREEN };
                    Frame::new()
                        .fill(theme::SURFACE)
                        .stroke(Stroke::new(1.0, theme::BORDER_STRONG))
                        .inner_margin(Margin::symmetric(14, 10))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(Vec2::new(3.0, 16.0), Sense::hover());
                                ui.painter().rect_filled(r, 0.0, c);
                                ui.label(&t.text);
                            });
                        });
                    ui.add_space(6.0);
                }
            });
        ctx.request_repaint_after(Duration::from_millis(250));
    }

    fn busy_overlay(&self, ctx: &egui::Context) {
        let Some((msg, p)) = &self.busy else { return };
        let screen = ctx.content_rect();
        egui::Area::new("busy-veil".into())
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (r, _) = ui.allocate_exact_size(screen.size(), Sense::click_and_drag());
                ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(150));
            });
        egui::Area::new("busy".into())
            .order(egui::Order::Tooltip)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                Frame::new()
                    .fill(theme::SURFACE)
                    .stroke(Stroke::new(1.0, theme::BORDER_STRONG))
                    .inner_margin(Margin::same(22))
                    .show(ui, |ui| {
                        ui.set_width(360.0);
                        ui.horizontal(|ui| {
                            ui.add(egui::Spinner::new().size(16.0).color(theme::ACCENT));
                            ui.label(RichText::new(msg).font(theme::heading(16.0)));
                        });
                        ui.add_space(12.0);
                        if let Some(p) = p {
                            progress_bar(ui, *p);
                            ui.add_space(4.0);
                            ui.label(
                                RichText::new(format!("{:.0}%", p * 100.0)).font(theme::mono(10.5)).color(theme::TEXT_DIM),
                            );
                        } else {
                            ui.label(RichText::new("Keep the keyboard connected.").color(theme::TEXT_DIM));
                        }
                    });
            });
    }
}

pub fn progress_bar(ui: &mut egui::Ui, p: f32) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 4.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, theme::RAISED);
    let mut f = r;
    f.set_width(r.width() * p.clamp(0.0, 1.0));
    ui.painter().rect_filled(f, 0.0, theme::ACCENT);
}

fn divider(ui: &mut egui::Ui) {
    let h = ui.available_height().min(48.0);
    let (r, _) = ui.allocate_exact_size(Vec2::new(1.0, h), Sense::hover());
    ui.painter().rect_filled(r, 0.0, theme::BORDER);
}

#[derive(Clone, Copy)]
enum Glyph {
    Min,
    Max,
    Close,
    Reload,
}

/// Title-bar button with a vector glyph (crisp at any scale, no font needed).
fn window_button(ui: &mut egui::Ui, glyph: Glyph, close: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(Vec2::new(46.0, ui.available_height()), Sense::click());
    if resp.hovered() {
        let c = if close { theme::RED } else { theme::HOVER };
        ui.painter().rect_filled(r, 0.0, c);
    }
    let col = if resp.hovered() { theme::TEXT } else { theme::TEXT_DIM };
    let s = Stroke::new(1.2, col);
    let c = r.center();
    let p = ui.painter();
    match glyph {
        Glyph::Min => {
            p.line_segment([c + Vec2::new(-5.5, 0.0), c + Vec2::new(5.5, 0.0)], s);
        }
        Glyph::Max => {
            p.rect_stroke(egui::Rect::from_center_size(c, Vec2::splat(10.0)), 0.0, s, egui::StrokeKind::Inside);
        }
        Glyph::Close => {
            p.line_segment([c + Vec2::new(-5.0, -5.0), c + Vec2::new(5.0, 5.0)], s);
            p.line_segment([c + Vec2::new(5.0, -5.0), c + Vec2::new(-5.0, 5.0)], s);
        }
        Glyph::Reload => {
            let pts: Vec<egui::Pos2> = (0..=20)
                .map(|i| {
                    let a = 0.6 + i as f32 / 20.0 * 5.0;
                    c + Vec2::new(a.cos(), a.sin()) * 6.0
                })
                .collect();
            let end = *pts.last().unwrap();
            p.add(egui::Shape::line(pts, s));
            p.line_segment([end, end + Vec2::new(0.5, -3.5)], s);
            p.line_segment([end, end + Vec2::new(3.5, 0.5)], s);
        }
    }
    resp
}

/// Edge/corner resize handles for the undecorated window.
fn resize_edges(ctx: &egui::Context) {
    if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
        return;
    }
    let rect = ctx.content_rect();
    let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else { return };
    const M: f32 = 6.0;
    let (l, r) = (pos.x - rect.left() < M, rect.right() - pos.x < M);
    let (t, b) = (pos.y - rect.top() < M, rect.bottom() - pos.y < M);
    let dir = match (l, r, t, b) {
        (true, _, true, _) => Some((ResizeDirection::NorthWest, CursorIcon::ResizeNorthWest)),
        (_, true, true, _) => Some((ResizeDirection::NorthEast, CursorIcon::ResizeNorthEast)),
        (true, _, _, true) => Some((ResizeDirection::SouthWest, CursorIcon::ResizeSouthWest)),
        (_, true, _, true) => Some((ResizeDirection::SouthEast, CursorIcon::ResizeSouthEast)),
        (true, ..) => Some((ResizeDirection::West, CursorIcon::ResizeWest)),
        (_, true, ..) => Some((ResizeDirection::East, CursorIcon::ResizeEast)),
        (_, _, true, _) => Some((ResizeDirection::North, CursorIcon::ResizeNorth)),
        (.., true) => Some((ResizeDirection::South, CursorIcon::ResizeSouth)),
        _ => None,
    };
    if let Some((d, c)) = dir {
        ctx.set_cursor_icon(c);
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(ViewportCommand::BeginResize(d));
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Runs even while hidden, so tray and device events are never missed.
        self.pump(ctx);
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        let ctx = &ctx;
        Panel::top("title")
            .exact_size(48.0)
            .frame(Frame::new().fill(theme::BG).stroke(Stroke::new(1.0, theme::BORDER)))
            .show(root, |ui| self.title_bar(ui));
        Panel::bottom("footer")
            .exact_size(30.0)
            .frame(Frame::new().fill(theme::BG).stroke(Stroke::new(1.0, theme::BORDER)))
            .show(root, |ui| self.footer(ui));

        let mut notes = Vec::new();
        CentralPanel::default()
            .frame(Frame::new().fill(theme::BG).inner_margin(Margin::symmetric(24, 20)))
            .show(root, |ui| {
                let Some(snap) = self.snap.as_mut() else {
                    self.empty_state(ui);
                    return;
                };
                let mut cx = ui::Ctx {
                    snap,
                    tx: &self.dev.tx,
                    store: &mut self.store,
                    busy: self.busy.is_some(),
                    notes: &mut notes,
                };
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.page {
                    Page::Keys => self.keys.ui(ui, &mut cx),
                    Page::Lighting => self.lighting.ui(ui, &mut cx),
                    Page::Macros => self.macros.ui(ui, &mut cx),
                    Page::Screen => self.screen.ui(ui, &mut cx),
                    Page::Settings => self.settings.ui(ui, &mut cx),
                    Page::Firmware => self.firmware.ui(ui, &mut cx),
                });
            });
        for n in notes {
            self.toast(n, false);
        }

        self.toasts(ctx);
        self.busy_overlay(ctx);
        resize_edges(ctx);
        if self.busy.is_some() || self.screen.animating() {
            ctx.request_repaint_after(Duration::from_millis(50));
        }
        let _ = self.hidden;
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.store.save();
    }
}
