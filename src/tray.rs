//! StatusNotifierItem tray icon with the original Show / Hide / Quit menu.

use std::sync::mpsc::Sender;

use ksni::blocking::TrayMethods;

pub enum TrayEvent {
    Show,
    Hide,
    Quit,
}

struct Tray {
    tx: Sender<TrayEvent>,
    wake: Box<dyn Fn() + Send>,
    icon: ksni::Icon,
}

impl Tray {
    fn emit(&self, e: TrayEvent) {
        let _ = self.tx.send(e);
        (self.wake)();
    }
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "k86-studio".into()
    }

    fn title(&self) -> String {
        "Attack Shark K86".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![self.icon.clone()]
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.emit(TrayEvent::Show);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        vec![
            StandardItem {
                label: "Show".into(),
                activate: Box::new(|t: &mut Self| t.emit(TrayEvent::Show)),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Hide".into(),
                activate: Box::new(|t: &mut Self| t.emit(TrayEvent::Hide)),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|t: &mut Self| t.emit(TrayEvent::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

fn icon() -> ksni::Icon {
    let img = image::load_from_memory(include_bytes!("../assets/brand/app.png"))
        .map(|i| i.resize(32, 32, image::imageops::FilterType::Triangle).to_rgba8())
        .unwrap_or_default();
    // SNI wants ARGB32 in network byte order.
    let mut data = Vec::with_capacity(img.len());
    for p in img.pixels() {
        data.extend_from_slice(&[p[3], p[0], p[1], p[2]]);
    }
    ksni::Icon { width: img.width() as i32, height: img.height() as i32, data }
}

/// Starts the tray; returns `false` when no StatusNotifier host is running.
pub fn spawn(tx: Sender<TrayEvent>, wake: impl Fn() + Send + 'static) -> bool {
    let tray = Tray { tx, wake: Box::new(wake), icon: icon() };
    match tray.spawn() {
        Ok(handle) => {
            // Keep the service alive for the life of the process.
            std::mem::forget(handle);
            true
        }
        Err(_) => false,
    }
}
