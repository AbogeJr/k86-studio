//! Visual language borrowed from ArtCraft Launcher: near-black surfaces,
//! hairline borders, square corners, Geist Mono uppercase labels, Archivo
//! headings and Inter body text.

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, RichText,
    Stroke, TextStyle, Vec2, Visuals,
};
use std::sync::Arc;

pub const BG: Color32 = Color32::from_rgb(0x0E, 0x0F, 0x11);
pub const SURFACE: Color32 = Color32::from_rgb(0x13, 0x14, 0x17);
pub const RAISED: Color32 = Color32::from_rgb(0x1A, 0x1B, 0x1F);
pub const HOVER: Color32 = Color32::from_rgb(0x22, 0x24, 0x28);
pub const BORDER: Color32 = Color32::from_rgb(0x2A, 0x2C, 0x31);
pub const BORDER_STRONG: Color32 = Color32::from_rgb(0x3A, 0x3D, 0x43);
pub const TEXT: Color32 = Color32::from_rgb(0xE8, 0xE8, 0xEA);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x9A, 0x9C, 0xA2);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(0x62, 0x65, 0x6C);
pub const PRIMARY: Color32 = Color32::from_rgb(0xD6, 0xD6, 0xD6);
pub const ON_PRIMARY: Color32 = Color32::from_rgb(0x0B, 0x0C, 0x0D);
pub const ACCENT: Color32 = Color32::from_rgb(0xFF, 0x8A, 0x1F);
pub const GREEN: Color32 = Color32::from_rgb(0x3D, 0xB8, 0x6B);
pub const AMBER: Color32 = Color32::from_rgb(0xD6, 0x9E, 0x2E);
pub const RED: Color32 = Color32::from_rgb(0xE5, 0x4D, 0x42);

pub const MONO: &str = "mono";
pub const MONO_BOLD: &str = "mono-bold";
pub const HEADING: &str = "heading";
pub const BODY_BOLD: &str = "body-bold";

pub fn mono(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MONO.into()))
}

pub fn mono_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MONO_BOLD.into()))
}

pub fn heading(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(HEADING.into()))
}

pub fn body_bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BODY_BOLD.into()))
}

/// Small uppercase, letter-spaced mono label (the launcher's signature).
pub fn label(text: &str) -> RichText {
    RichText::new(spaced(text)).font(mono(10.5)).color(TEXT_DIM)
}

pub fn spaced(text: &str) -> String {
    text.to_uppercase()
}

fn add_font(fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]) {
    fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    add_font(&mut fonts, "inter", include_bytes!("../assets/fonts/inter-400.ttf"));
    add_font(&mut fonts, "inter-600", include_bytes!("../assets/fonts/inter-600.ttf"));
    add_font(&mut fonts, "archivo", include_bytes!("../assets/fonts/archivo-700.ttf"));
    add_font(&mut fonts, "geist", include_bytes!("../assets/fonts/geist-mono-500.ttf"));
    add_font(&mut fonts, "geist-600", include_bytes!("../assets/fonts/geist-mono-600.ttf"));

    // The bundled fonts are Latin subsets; borrow symbols (arrows, ●, ✕) from
    // a system font that every desktop Linux ships.
    let mut fallback = Vec::new();
    for path in [
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansSymbols2-Regular.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            let name = format!("sys:{path}");
            fonts.font_data.insert(name.clone(), Arc::new(FontData::from_owned(bytes)));
            fallback.push(name);
        }
    }
    fallback.extend(fonts.families[&FontFamily::Proportional].iter().cloned());
    let with = |first: &str| {
        let mut v = vec![first.to_owned()];
        v.extend(fallback.iter().cloned());
        v
    };
    fonts.families.insert(FontFamily::Proportional, with("inter"));
    fonts.families.insert(FontFamily::Monospace, with("geist"));
    fonts.families.insert(FontFamily::Name(MONO.into()), with("geist"));
    fonts.families.insert(FontFamily::Name(MONO_BOLD.into()), with("geist-600"));
    fonts.families.insert(FontFamily::Name(HEADING.into()), with("archivo"));
    fonts.families.insert(FontFamily::Name(BODY_BOLD.into()), with("inter-600"));
    ctx.set_fonts(fonts);

    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, FontId::proportional(11.5)),
            (TextStyle::Body, FontId::proportional(13.5)),
            (TextStyle::Button, mono_bold(11.0)),
            (TextStyle::Heading, heading(20.0)),
            (TextStyle::Monospace, mono(12.0)),
        ]
        .into();
        style.spacing.item_spacing = Vec2::new(8.0, 8.0);
        style.spacing.button_padding = Vec2::new(14.0, 7.0);
        style.spacing.interact_size = Vec2::new(36.0, 30.0);
        style.spacing.slider_width = 220.0;
        style.spacing.combo_width = 200.0;
        style.spacing.window_margin = Margin::same(20);
        style.spacing.menu_margin = Margin::same(6);
        style.spacing.scroll.bar_width = 6.0;
        style.spacing.scroll.floating = true;
        style.interaction.selectable_labels = false;

        let mut v = Visuals::dark();
        let square = CornerRadius::ZERO;
        v.override_text_color = Some(TEXT);
        v.panel_fill = BG;
        v.window_fill = SURFACE;
        v.window_stroke = Stroke::new(1.0, BORDER_STRONG);
        v.window_corner_radius = square;
        v.window_shadow = egui::epaint::Shadow { offset: [0, 12], blur: 32, spread: 0, color: Color32::from_black_alpha(140) };
        v.popup_shadow = egui::epaint::Shadow { offset: [0, 6], blur: 16, spread: 0, color: Color32::from_black_alpha(120) };
        v.menu_corner_radius = square;
        v.extreme_bg_color = Color32::from_rgb(0x08, 0x09, 0x0A);
        v.faint_bg_color = SURFACE;
        v.code_bg_color = RAISED;
        v.selection.bg_fill = Color32::from_rgb(0xC8, 0x62, 0x0E);
        v.selection.stroke = Stroke::new(1.0, ACCENT);
        v.hyperlink_color = ACCENT;
        v.warn_fg_color = AMBER;
        v.error_fg_color = RED;
        v.slider_trailing_fill = true;
        v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.5 };

        let w = &mut v.widgets;
        w.noninteractive.bg_fill = SURFACE;
        w.noninteractive.weak_bg_fill = SURFACE;
        w.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        w.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_DIM);
        w.inactive.bg_fill = RAISED;
        w.inactive.weak_bg_fill = SURFACE;
        w.inactive.bg_stroke = Stroke::new(1.0, BORDER);
        w.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        w.hovered.bg_fill = HOVER;
        w.hovered.weak_bg_fill = HOVER;
        w.hovered.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
        w.hovered.fg_stroke = Stroke::new(1.0, TEXT);
        w.hovered.expansion = 0.0;
        w.active.bg_fill = PRIMARY;
        w.active.weak_bg_fill = PRIMARY;
        w.active.bg_stroke = Stroke::new(1.0, PRIMARY);
        w.active.fg_stroke = Stroke::new(1.0, ON_PRIMARY);
        w.active.expansion = 0.0;
        w.open.bg_fill = RAISED;
        w.open.weak_bg_fill = RAISED;
        w.open.bg_stroke = Stroke::new(1.0, BORDER_STRONG);
        for s in [&mut w.noninteractive, &mut w.inactive, &mut w.hovered, &mut w.active, &mut w.open] {
            s.corner_radius = square;
        }
        style.visuals = v;
    });
}

/// Filled light button (ArtCraft "INSTALL").
pub fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let b = egui::Button::new(RichText::new(spaced(text)).font(mono_bold(11.0)).color(ON_PRIMARY))
        .fill(PRIMARY)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::ZERO)
        .min_size(Vec2::new(0.0, 32.0));
    ui.add(b)
}

/// Outlined button (ArtCraft "LATER").
pub fn ghost_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let b = egui::Button::new(RichText::new(spaced(text)).font(mono_bold(11.0)).color(TEXT))
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::new(1.0, BORDER_STRONG))
        .corner_radius(CornerRadius::ZERO)
        .min_size(Vec2::new(0.0, 32.0));
    ui.add(b)
}

pub fn danger_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let b = egui::Button::new(RichText::new(spaced(text)).font(mono_bold(11.0)).color(RED))
        .fill(Color32::TRANSPARENT)
        .stroke(Stroke::new(1.0, RED.gamma_multiply(0.6)))
        .corner_radius(CornerRadius::ZERO)
        .min_size(Vec2::new(0.0, 32.0));
    ui.add(b)
}

/// Segmented tab/chip: filled when selected, plain mono text otherwise.
pub fn chip(ui: &mut egui::Ui, selected: bool, text: &str) -> egui::Response {
    let (fg, fill) = if selected { (ON_PRIMARY, PRIMARY) } else { (TEXT_DIM, Color32::TRANSPARENT) };
    let b = egui::Button::new(RichText::new(spaced(text)).font(mono_bold(10.5)).color(fg))
        .fill(fill)
        .stroke(Stroke::NONE)
        .corner_radius(CornerRadius::ZERO)
        .min_size(Vec2::new(0.0, 26.0));
    ui.add(b)
}

/// Outlined status pill (ArtCraft "AVAILABLE TO INSTALL").
pub fn pill(ui: &mut egui::Ui, text: &str, color: Color32) -> egui::Response {
    let galley = ui.painter().layout_no_wrap(spaced(text), mono_bold(9.5), color);
    let size = galley.size() + Vec2::new(12.0, 6.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_stroke(rect, 0.0, Stroke::new(1.0, color.gamma_multiply(0.7)), egui::StrokeKind::Inside);
    ui.painter().galley(rect.min + Vec2::new(6.0, 3.0), galley, color);
    resp
}

/// Numbered square badge (ArtCraft "01").
pub fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color)
        .inner_margin(Margin::symmetric(6, 2))
        .show(ui, |ui| ui.label(RichText::new(text).font(mono_bold(10.5)).color(Color32::WHITE)));
}

/// Bordered card with an optional header row.
pub fn card<R>(ui: &mut egui::Ui, title: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !title.is_empty() {
                ui.label(label(title));
                ui.add_space(6.0);
            }
            add(ui)
        })
        .inner
}

/// Two-tone wordmark: first part white, second in a colour.
pub fn two_tone(ui: &mut egui::Ui, a: &str, b: &str, color: Color32, size: f32) {
    let mut job = egui::text::LayoutJob::default();
    job.append(a, 0.0, egui::TextFormat::simple(heading(size), TEXT));
    job.append(b, 0.0, egui::TextFormat::simple(heading(size), color));
    ui.label(job);
}
