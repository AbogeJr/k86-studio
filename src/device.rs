//! Background worker that owns the keyboard. The UI sends `Command`s and
//! receives `Event`s, so slow HID traffic (and the firmware's 500 ms settle
//! times) never blocks a frame.

use std::cell::Cell;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};

use crate::hid::{self, HidDevice};
use crate::keymap::{self, Macro};
use crate::proto::{self, K86, LightMode, LightSetting, Rgb, ScreenFrame, SleepTimes};
use crate::screen;

pub const MACRO_SLOTS: u8 = 50;
/// Minimum quiet time after a write before the next command.
const WRITE_GAP: Duration = Duration::from_millis(1000);

#[derive(Clone)]
pub struct Snapshot {
    pub node: String,
    pub firmware: u16,
    pub screen_firmware: Option<(u16, u16)>,
    pub profile: u8,
    pub debounce: u8,
    pub sleep: SleepTimes,
    pub light: LightSetting,
    pub keys: [Vec<u8>; proto::LAYERS as usize],
    /// The single (Windows) Fn layer.
    pub fn_keys: Vec<u8>,
    pub macros: Vec<(u8, Macro)>,
}

pub enum Command {
    Reload,
    SetProfile(u8),
    SetLight(LightSetting),
    /// Switches to custom pattern `pattern` (0..=2) and writes its colours.
    WriteUserPicture { pattern: u8, colors: Vec<Rgb> },
    /// Switches to custom pattern `pattern` and reads its colours back.
    ReadUserPicture { pattern: u8 },
    SetKey { layer: u8, slot: u8, value: [u8; 4], fn_layer: bool },
    /// Rewrites a whole layer (used for "restore defaults").
    SetMatrix { layer: u8, matrix: Vec<u8>, fn_layer: bool },
    SetMacro { index: u8, data: Macro },
    SetDebounce(u8),
    SetSleep(SleepTimes),
    SyncClock,
    EraseScreen,
    /// Static image into slot 0..=4, or an animation when more than one frame.
    UploadScreen { frames: Vec<screen::Frame>, slot: u8, delay: u8 },
    Firmware(crate::firmware::Package),
    FactoryReset,
}

pub enum Event {
    Searching,
    /// A keyboard is present but cannot be opened (permissions).
    NoAccess(String),
    Connected(Box<Snapshot>),
    Disconnected,
    Busy(String),
    Progress(f32),
    Done(String),
    Failed(String),
    Profile(u8),
    Light(LightSetting),
    Keys { layer: u8, matrix: Vec<u8>, fn_layer: bool },
    UserPicture(Vec<Rgb>),
}

pub struct Handle {
    pub tx: Sender<Command>,
    pub rx: Receiver<Event>,
}

pub fn spawn(wake: impl Fn() + Send + 'static) -> Handle {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (ev_tx, ev_rx) = mpsc::channel();
    thread::Builder::new()
        .name("k86-device".into())
        .spawn(move || Worker { rx: cmd_rx, tx: ev_tx, wake: Box::new(wake), last_write: Cell::new(None) }.run())
        .expect("spawn device thread");
    Handle { tx: cmd_tx, rx: ev_rx }
}

struct Worker {
    rx: Receiver<Command>,
    tx: Sender<Event>,
    wake: Box<dyn Fn() + Send>,
    last_write: Cell<Option<Instant>>,
}

fn find() -> Option<hid::HidInfo> {
    hid::enumerate()
        .into_iter()
        .find(|d| d.vid == proto::VID && d.pid == proto::PID && d.usage_page == 0xFFFF)
}

impl Worker {
    fn emit(&self, e: Event) {
        let _ = self.tx.send(e);
        (self.wake)();
    }

    fn run(self) {
        let mut last_error = String::new();
        loop {
            self.emit(Event::Searching);
            let kb = loop {
                match find() {
                    Some(info) => match HidDevice::open(&info) {
                        Ok(dev) => break K86::new(dev),
                        Err(e) => {
                            let msg = format!("{}: {e}", info.node.display());
                            if msg != last_error {
                                self.emit(Event::NoAccess(msg.clone()));
                                last_error = msg;
                            }
                        }
                    },
                    None => last_error.clear(),
                }
                if self.drain_while_disconnected() {
                    return;
                }
            };
            last_error.clear();
            match self.load(&kb) {
                Ok(s) => self.emit(Event::Connected(Box::new(s))),
                Err(_) => {
                    thread::sleep(Duration::from_secs(1));
                    continue;
                }
            }
            if self.serve(&kb) {
                return;
            }
            self.emit(Event::Disconnected);
        }
    }

    /// Waits ~1 s for hotplug while rejecting commands. Returns true on shutdown.
    fn drain_while_disconnected(&self) -> bool {
        let until = Instant::now() + Duration::from_secs(1);
        loop {
            let left = until.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(_) => self.emit(Event::Failed("Keyboard is not connected".into())),
                Err(RecvTimeoutError::Timeout) => return false,
                Err(RecvTimeoutError::Disconnected) => return true,
            }
        }
    }

    fn load(&self, kb: &K86) -> Result<Snapshot> {
        if kb.who_am_i()? != proto::DEVICE_ID {
            bail!("unsupported keyboard");
        }
        let mut keys: [Vec<u8>; 3] = Default::default();
        for l in 0..proto::LAYERS {
            keys[l as usize] = kb.key_matrix(l)?;
        }
        let fn_keys = kb.fn_matrix(0)?;
        let mut macro_ids: Vec<u8> = keys
            .iter()
            .chain(std::iter::once(&fn_keys))
            .flat_map(|m| (0..proto::SLOTS).map(move |i| keymap::slot(m, i)))
            .filter(|b| b[0] == 9)
            .map(|b| b[2])
            .collect();
        macro_ids.sort_unstable();
        macro_ids.dedup();
        let mut macros = Vec::new();
        for id in macro_ids {
            macros.push((id, Macro::decode(&kb.macro_raw(id)?)));
        }
        let snap = Snapshot {
            node: kb.node().display().to_string(),
            firmware: kb.usb_version()?,
            screen_firmware: kb.oled_version().ok().flatten(),
            profile: kb.profile()?,
            debounce: kb.debounce()?,
            sleep: kb.sleep_times()?,
            light: kb.light()?,
            keys,
            fn_keys,
            macros,
        };
        // The original syncs the screen clock whenever it reads the keyboard.
        if kb.set_clock(chrono::Local::now().naive_local()).is_ok() {
            self.wrote();
        }
        Ok(snap)
    }

    /// Handles commands until the keyboard goes away. Returns true on shutdown.
    fn serve(&self, kb: &K86) -> bool {
        loop {
            match self.rx.recv_timeout(Duration::from_millis(1000)) {
                Ok(cmd) => {
                    if let Err(e) = self.exec(kb, cmd) {
                        self.emit(Event::Failed(e.to_string()));
                        if find().is_none() {
                            return false;
                        }
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    if find().is_none_or(|i| i.node.as_path() != kb.node()) {
                        return false;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => return true,
            }
        }
    }

    /// Waits until the keyboard has had `WRITE_GAP` since the last write.
    /// It commits settings to flash after a write, and traffic during that
    /// window can make it reset (the original app's 1 s sync loop hides this).
    fn pace(&self) {
        if let Some(t) = self.last_write.get() {
            if let Some(left) = WRITE_GAP.checked_sub(t.elapsed()) {
                thread::sleep(left);
            }
        }
    }

    fn wrote(&self) {
        self.last_write.set(Some(Instant::now()));
    }

    fn exec(&self, kb: &K86, cmd: Command) -> Result<()> {
        self.pace();
        match cmd {
            Command::Reload => {
                self.emit(Event::Busy("Reading keyboard".into()));
                let s = self.load(kb)?;
                self.emit(Event::Connected(Box::new(s)));
            }
            Command::SetProfile(p) => {
                kb.set_profile(p)?;
                self.wrote();
                self.emit(Event::Profile(p));
            }
            Command::SetLight(s) => {
                kb.set_light(&s)?;
                self.wrote();
                self.emit(Event::Light(s));
            }
            Command::WriteUserPicture { pattern, colors } => {
                self.emit(Event::Busy("Writing custom pattern".into()));
                let l = self.select_pattern(kb, pattern)?;
                kb.set_user_picture(&colors)?;
                self.wrote();
                self.emit(Event::Light(l));
                self.emit(Event::Done("Write successful".into()));
            }
            Command::ReadUserPicture { pattern } => {
                self.emit(Event::Busy("Reading custom pattern".into()));
                let l = self.select_pattern(kb, pattern)?;
                let c = kb.user_picture()?;
                self.emit(Event::Light(l));
                self.emit(Event::UserPicture(c));
                self.emit(Event::Done("Read successfully".into()));
            }
            Command::SetKey { layer, slot, value, fn_layer } => {
                if fn_layer {
                    kb.set_fn_key(layer, slot, value)?;
                } else {
                    self.activate(kb, layer)?;
                    kb.set_key(layer, slot, value)?;
                }
                self.wrote();
            }
            Command::SetMatrix { layer, matrix, fn_layer } => {
                self.emit(Event::Busy("Writing key map".into()));
                if fn_layer {
                    kb.set_fn_matrix(layer, &matrix)?;
                } else {
                    self.activate(kb, layer)?;
                    kb.set_key_matrix(layer, &matrix)?;
                }
                self.wrote();
                self.emit(Event::Keys { layer, matrix, fn_layer });
                self.emit(Event::Done("Key map saved".into()));
            }
            Command::SetMacro { index, data } => {
                kb.set_macro_raw(index, &data.encode())?;
                self.wrote();
                self.emit(Event::Done("Macro saved".into()));
            }
            Command::SetDebounce(v) => {
                kb.set_debounce(v)?;
                self.wrote();
            }
            Command::SetSleep(t) => {
                kb.set_sleep_times(t)?;
                self.wrote();
            }
            Command::SyncClock => {
                kb.set_clock(chrono::Local::now().naive_local())?;
                self.wrote();
                self.emit(Event::Done("Screen clock synced".into()));
            }
            Command::EraseScreen => {
                self.erase(kb)?;
                self.wrote();
            }
            Command::UploadScreen { frames, slot, delay } => {
                self.upload(kb, &frames, slot, delay)?;
                self.wrote();
            }
            Command::Firmware(pkg) => {
                self.emit(Event::Busy("Updating firmware. Do not unplug the keyboard".into()));
                crate::firmware::flash(kb, &pkg, |p| self.emit(Event::Progress(p)))?;
                self.wrote();
                self.emit(Event::Done("Firmware updated. The keyboard will restart.".into()));
            }
            Command::FactoryReset => {
                self.emit(Event::Busy("Restoring factory settings".into()));
                kb.reset()?;
                let s = self.load(kb)?;
                self.emit(Event::Connected(Box::new(s)));
                self.emit(Event::Done("Factory settings restored".into()));
            }
        }
        Ok(())
    }

    /// Switches the backlight to custom pattern `pattern` (0..=2).
    fn select_pattern(&self, kb: &K86, pattern: u8) -> Result<LightSetting> {
        let mut l = kb.light()?;
        l.mode = LightMode::USER_PICTURE;
        l.option = pattern;
        kb.set_light(&l)?;
        self.wrote();
        self.pace();
        Ok(l)
    }

    /// Key-matrix writes land on the active profile regardless of the layer
    /// byte, so make `layer` active first (the original UI does the same).
    fn activate(&self, kb: &K86, layer: u8) -> Result<()> {
        if kb.profile()? != layer {
            kb.set_profile(layer)?;
            self.wrote();
            self.emit(Event::Profile(layer));
            self.pace();
        }
        Ok(())
    }

    fn upload(&self, kb: &K86, frames: &[screen::Frame], slot: u8, delay: u8) -> Result<()> {
        if frames.is_empty() {
            bail!("Nothing to upload");
        }
        let animated = frames.len() > 1;
        self.emit(Event::Busy(if animated { "Writing animation" } else { "Writing image" }.into()));
        let rect = screen::union_bbox(frames);
        let data: Vec<Vec<u8>> = frames.iter().map(|f| screen::encode(f, rect)).collect();
        let n = frames.len();
        let frame = |i: usize| ScreenFrame {
            current_frame: if animated { i as u8 } else { slot },
            frame_num: n as u8,
            frame_delay: if animated { delay } else { 0 },
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
            data: &data[i],
        };
        if !kb.screen_begin(&frame(0), 0)? {
            bail!("The screen did not accept the upload");
        }
        for i in 0..n {
            kb.screen_data(&frame(i), |p| self.emit(Event::Progress((i as f32 + p) / n as f32)))?;
        }
        self.emit(Event::Done(if animated { "Animation written" } else { "Image written" }.into()));
        Ok(())
    }

    fn erase(&self, kb: &K86) -> Result<()> {
        self.emit(Event::Busy("Clearing screen".into()));
        let watch = crate::firmware::InputWatch::open(proto::VID, proto::PID);
        if !kb.erase_screen_flash()? {
            bail!("The screen could not be cleared");
        }
        // The keyboard reports `[44, 0, 0]` when done; the original expects ~55 s.
        let start = Instant::now();
        let expected = Duration::from_secs(55);
        loop {
            if watch.as_ref().is_some_and(|w| w.saw(&[44, 0, 0])) {
                break;
            }
            let t = start.elapsed();
            if t > Duration::from_secs(180) {
                break;
            }
            self.emit(Event::Progress((t.as_secs_f32() / expected.as_secs_f32()).min(0.99)));
            thread::sleep(Duration::from_millis(550));
        }
        self.emit(Event::Done("Screen cleared".into()));
        Ok(())
    }
}

#[cfg(test)]
mod hardware {
    use super::*;
    use crate::keymap;

    fn wait_for(h: &Handle, mut want: impl FnMut(&Event) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(60);
        while Instant::now() < deadline {
            if let Ok(e) = h.rx.recv_timeout(Duration::from_millis(200)) {
                if let Event::Failed(m) = &e {
                    panic!("worker failed: {m}");
                }
                if want(&e) {
                    return;
                }
            }
        }
        panic!("timed out");
    }

    /// Drives the real worker through edits and restores, with pacing.
    #[test]
    #[ignore]
    fn worker_round_trip() {
        let h = spawn(|| {});
        let mut snap = None;
        wait_for(&h, |e| {
            if let Event::Connected(s) = e {
                snap = Some((**s).clone());
                true
            } else {
                false
            }
        });
        let s = snap.unwrap();
        let slot = keymap::slot_of_hid(4, 0).unwrap() as u8;
        let mut l = s.light;
        l.bright = if s.light.bright == 1 { 2 } else { 1 };
        h.tx.send(Command::SetLight(l)).unwrap();
        h.tx.send(Command::SetLight(s.light)).unwrap();
        // Profile 2 edit forces a profile switch first.
        let orig2 = keymap::slot(&s.keys[1], slot as usize);
        h.tx.send(Command::SetKey { layer: 1, slot, value: [0, 0, 5, 0], fn_layer: false }).unwrap();
        h.tx.send(Command::SetKey { layer: 1, slot, value: orig2, fn_layer: false }).unwrap();
        h.tx.send(Command::SetKey { layer: 0, slot, value: [0, 0, 4, 0], fn_layer: false }).unwrap();
        let fn_orig = keymap::slot(&s.fn_keys, slot as usize);
        h.tx.send(Command::SetKey { layer: 0, slot, value: [3, 0, 233, 0], fn_layer: true }).unwrap();
        h.tx.send(Command::SetKey { layer: 0, slot, value: fn_orig, fn_layer: true }).unwrap();
        h.tx.send(Command::ReadUserPicture { pattern: 0 }).unwrap();
        wait_for(&h, |e| matches!(e, Event::UserPicture(_)));
        h.tx.send(Command::SetLight(s.light)).unwrap();
        h.tx.send(Command::SetProfile(s.profile)).unwrap();
        h.tx.send(Command::Reload).unwrap();
        let mut after = None;
        wait_for(&h, |e| {
            if let Event::Connected(s) = e {
                after = Some((**s).clone());
                true
            } else {
                false
            }
        });
        let a = after.unwrap();
        assert_eq!(a.keys, s.keys);
        assert_eq!(a.fn_keys, s.fn_keys);
        assert_eq!(a.profile, s.profile);
        assert_eq!((a.light.mode, a.light.bright, a.light.speed), (s.light.mode, s.light.bright, s.light.speed));
    }
}
