mod app;
mod autostart;
mod defaults;
mod device;
mod firmware;
mod hid;
mod keymap;
mod layout;
mod proto;
mod screen;
mod store;
mod theme;
mod tray;
mod ui;

use std::io::{Read, Write};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
use std::sync::{mpsc, Arc, OnceLock};

use eframe::egui;

use tray::TrayEvent;

/// Abstract socket used to hand a second launch over to the running instance.
const INSTANCE: &[u8] = b"k86-studio-instance";

fn main() -> eframe::Result {
    let hidden = std::env::args().any(|a| a == "--hidden");

    let addr = SocketAddr::from_abstract_name(INSTANCE).expect("socket name");
    let listener = match UnixListener::bind_addr(&addr) {
        Ok(l) => l,
        Err(_) => {
            // Already running: ask it to come to the front.
            if let Ok(mut s) = UnixStream::connect_addr(&addr) {
                let _ = s.write_all(b"show");
            }
            return Ok(());
        }
    };

    let (tx, rx) = mpsc::channel::<TrayEvent>();
    let ctx_cell: Arc<OnceLock<egui::Context>> = Arc::new(OnceLock::new());
    {
        let tx = tx.clone();
        let ctx_cell = ctx_cell.clone();
        std::thread::spawn(move || {
            for mut conn in listener.incoming().flatten() {
                let mut buf = [0u8; 8];
                let _ = conn.read(&mut buf);
                let _ = tx.send(TrayEvent::Show);
                if let Some(c) = ctx_cell.get() {
                    c.request_repaint();
                }
            }
        });
    }

    let icon = image::load_from_memory(include_bytes!("../assets/brand/icon.png"))
        .map(|i| {
            let i = i.resize(256, 256, image::imageops::FilterType::Lanczos3).to_rgba8();
            egui::IconData { width: i.width(), height: i.height(), rgba: i.into_raw() }
        })
        .ok();
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("Attack Shark K86")
        .with_app_id("k86-studio")
        .with_inner_size([1240.0, 820.0])
        .with_min_inner_size([980.0, 680.0])
        .with_decorations(false)
        .with_visible(!hidden);
    if let Some(i) = icon {
        viewport = viewport.with_icon(i);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        "k86-studio",
        options,
        Box::new(move |cc| {
            let _ = ctx_cell.set(cc.egui_ctx.clone());
            Ok(Box::new(app::App::new(cc, hidden, tx, rx)))
        }),
    )
}
