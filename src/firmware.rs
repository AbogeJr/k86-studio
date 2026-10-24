//! Firmware updates: cloud version check, package download, and flashing.
//!
//! The original Linux build could not flash at all (it relied on the
//! Windows/macOS "IOT" helper to reach the bootloader). Here the bootloader is
//! opened directly. No firmware byte is sent unless the bootloader has been
//! found and has answered the start handshake.

use std::fs::OpenOptions;
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;

use crate::hid::{self, HidDevice};
use crate::proto::{self, byte_sum, Checksum, K86};

const API: &str = "https://api2.rongyuan.tech:3816/api/v2/get_fw_version";
const DOWNLOAD: &str = "https://api2.rongyuan.tech:3816/download/";
/// The bootloader image starts after this many header bytes.
const SKIP: usize = 0x10000;
/// Bootloader PID for this keyboard family (normal PID + 1).
const BOOT_PID: u16 = 0x4016;
const BOOT_USAGE_PAGE: u16 = 0xFF01;

#[derive(Deserialize)]
struct ApiReply {
    code: i64,
    #[serde(default)]
    data: Option<ApiData>,
    #[serde(default)]
    err_message: Option<String>,
}

#[derive(Deserialize, Clone, Debug)]
pub struct ApiData {
    pub version_str: String,
    pub file_path: String,
}

#[derive(Clone, Debug)]
pub struct Update {
    pub info: ApiData,
    /// Newer main firmware, as a device-comparable number.
    pub main: Option<u16>,
    /// Newer screen (OLED) firmware.
    pub oled: Option<u16>,
}

impl Update {
    pub fn any(&self) -> bool {
        self.main.is_some() || self.oled.is_some()
    }
}

/// Firmware images unpacked from a server package.
#[derive(Clone, Default)]
pub struct Package {
    pub main: Option<Vec<u8>>,
    pub oled: Option<Vec<u8>>,
}

/// "V1.05" / "105" style segment -> digits read as hex (the original's rule).
fn seg_version(seg: &str) -> Option<u16> {
    let digits: String = seg.chars().filter(|c| c.is_ascii_digit()).collect();
    u16::from_str_radix(&digits, 16).ok()
}

fn segments(v: &str) -> Vec<&str> {
    v.split('_').filter(|s| !s.is_empty()).collect()
}

pub fn display_version(v: u16) -> String {
    format!("{:x}", v)
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .into()
}

pub fn check(device_fw: u16, oled_fw: Option<u16>) -> Result<Update> {
    let reply: ApiReply = agent()
        .post(API)
        .config()
        .timeout_global(Some(Duration::from_secs(8)))
        .build()
        .send_json(serde_json::json!({ "dev_id": proto::DEVICE_ID }))
        .context("could not reach the update server")?
        .body_mut()
        .read_json()?;
    if reply.code != 0 {
        bail!(reply.err_message.unwrap_or_else(|| "the update server returned an error".into()));
    }
    let info = reply.data.ok_or_else(|| anyhow!("no firmware is published for this keyboard"))?;
    let segs = segments(&info.version_str);
    let main = segs.first().and_then(|s| seg_version(s)).filter(|&v| v > device_fw);
    let oled = segs
        .iter()
        .skip(1)
        .find(|s| s.to_ascii_lowercase().contains("oledv"))
        .and_then(|s| seg_version(s))
        .filter(|&v| v > oled_fw.unwrap_or(0));
    Ok(Update { info, main, oled })
}

pub fn download(u: &Update, mut progress: impl FnMut(f32)) -> Result<Package> {
    let mut resp = agent().get(&format!("{DOWNLOAD}{}", u.info.file_path)).call()?;
    let total = resp.body().content_length().unwrap_or(0) as usize;
    let mut blob = Vec::with_capacity(total);
    let mut reader = resp.body_mut().with_config().limit(64 * 1024 * 1024).reader();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        blob.extend_from_slice(&buf[..n]);
        if total > 0 {
            progress(blob.len() as f32 / total as f32);
        }
    }
    let mut pkg = Package::default();
    if segments(&u.info.version_str).len() > 1 {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(blob))?;
        let mut take = |name: &str| -> Option<Vec<u8>> {
            let mut f = zip.by_name(name).ok()?;
            let mut v = Vec::new();
            f.read_to_end(&mut v).ok()?;
            Some(v)
        };
        pkg.main = take("firmwareFile.bin");
        pkg.oled = take("firmwareOledFile.bin");
    } else {
        let mut v = Vec::new();
        flate2::read::DeflateDecoder::new(&blob[..]).read_to_end(&mut v)?;
        pkg.main = Some(v);
    }
    if u.main.is_none() {
        pkg.main = None;
    }
    if u.oled.is_none() {
        pkg.oled = None;
    }
    Ok(pkg)
}

/// True when hidraw nodes of every Attack Shark PID are accessible (the udev
/// rule shipped with the driver). Required before rebooting into the bootloader.
pub fn udev_rule_installed() -> bool {
    ["/etc/udev/rules.d", "/usr/lib/udev/rules.d", "/lib/udev/rules.d"].iter().any(|d| {
        std::fs::read_dir(d).into_iter().flatten().flatten().any(|e| {
            std::fs::read_to_string(e.path())
                .map(|t| t.contains("3151") && t.contains("hidraw"))
                .unwrap_or(false)
        })
    })
}

fn find_boot() -> Option<hid::HidInfo> {
    hid::enumerate()
        .into_iter()
        .find(|d| d.vid == proto::VID && d.pid == BOOT_PID && d.usage_page == BOOT_USAGE_PAGE)
}

/// Flashes a package in the original order: main firmware, then (after the
/// keyboard restarts) the screen controller.
pub fn flash(kb: &K86, pkg: &Package, mut progress: impl FnMut(f32)) -> Result<()> {
    let parts = pkg.main.is_some() as usize + pkg.oled.is_some() as usize;
    if parts == 0 {
        bail!("nothing to flash");
    }
    let mut done = 0usize;
    let mut reopened = None;
    if let Some(main) = &pkg.main {
        flash_main(kb, main, |p| progress((done as f32 + p) / parts as f32))?;
        done += 1;
        if pkg.oled.is_some() {
            reopened = Some(wait_for_keyboard()?);
            sleep(Duration::from_secs(3));
        }
    }
    if let Some(oled) = &pkg.oled {
        let kb = reopened.as_ref().unwrap_or(kb);
        flash_oled(kb, oled, |p| progress((done as f32 + p) / parts as f32))?;
    }
    progress(1.0);
    Ok(())
}

fn wait_for_keyboard() -> Result<K86> {
    let deadline = Instant::now() + Duration::from_secs(60);
    while Instant::now() < deadline {
        sleep(Duration::from_millis(500));
        let Some(info) = hid::enumerate()
            .into_iter()
            .find(|d| d.vid == proto::VID && d.pid == proto::PID && d.usage_page == 0xFFFF)
        else {
            continue;
        };
        if let Ok(dev) = HidDevice::open(&info) {
            let kb = K86::new(dev);
            if kb.who_am_i().is_ok() {
                return Ok(kb);
            }
        }
    }
    bail!("the keyboard did not come back after the main firmware update; replug it and update the screen firmware again")
}

fn flash_main(kb: &K86, fw: &[u8], mut progress: impl FnMut(f32)) -> Result<()> {
    if fw.len() <= SKIP {
        bail!("the firmware image is too small ({} bytes)", fw.len());
    }
    if !udev_rule_installed() {
        bail!("install 70-attackshark.rules first: the bootloader would not be accessible");
    }
    let app = &fw[SKIP..];
    let chunks = app.len().div_ceil(64);
    let len = app.len() as u32;

    progress(0.0);
    kb.enter_bootloader()?;
    sleep(Duration::from_secs(1));
    let deadline = Instant::now() + Duration::from_secs(50);
    let info = loop {
        if let Some(i) = find_boot() {
            break i;
        }
        if Instant::now() > deadline {
            bail!("the keyboard did not enter its bootloader; unplug and replug it, nothing was written");
        }
        sleep(Duration::from_millis(500));
    };
    if hid::has_report_ids(&info) {
        bail!("unexpected bootloader interface on {}; unplug and replug the keyboard, nothing was written", info.node.display());
    }
    // udev applies permissions shortly after the node appears.
    let boot = {
        let mut tries = 0;
        loop {
            match HidDevice::open(&info) {
                Ok(d) => break d,
                Err(e) if tries < 20 => {
                    let _ = e;
                    tries += 1;
                    sleep(Duration::from_millis(250));
                }
                Err(e) => bail!("cannot open the bootloader ({}): {e}; nothing was written", info.node.display()),
            }
        }
    };

    let send = |data: &[u8]| -> Result<()> {
        sleep(Duration::from_millis(10));
        let mut v = data.to_vec();
        proto::apply_checksum(&mut v, Checksum::None);
        boot.set_feature(&v)?;
        Ok(())
    };

    let start = [
        0xBA,
        0xC0,
        chunks as u8,
        (chunks >> 8) as u8,
        len as u8,
        (len >> 8) as u8,
        (len >> 16) as u8,
    ];
    let mut acked = false;
    for _ in 0..10 {
        if send(&start).is_ok() {
            sleep(Duration::from_millis(10));
            if boot.get_feature().is_ok() {
                acked = true;
                break;
            }
        }
        sleep(Duration::from_secs(1));
    }
    if !acked {
        bail!("the bootloader did not answer; unplug and replug the keyboard, nothing was written");
    }

    for (i, c) in app.chunks(64).enumerate() {
        send(c).with_context(|| format!("write failed at block {i} of {chunks}; do not unplug, retry the update"))?;
        progress(i as f32 / chunks as f32);
    }
    let sum = byte_sum(app);
    let mut end = vec![0xBA, 0xC2, chunks as u8, (chunks >> 8) as u8];
    end.extend_from_slice(&sum.to_le_bytes());
    end.extend_from_slice(&len.to_le_bytes());
    send(&end)?;
    sleep(Duration::from_millis(100));
    let _ = boot.get_feature();
    progress(1.0);
    Ok(())
}

mod oled_cmd {
    pub const SET_BOOTLOADER: u8 = 48;
    pub const GET_BOOTLOADER: u8 = 176;
    pub const SET_BOOTSTART: u8 = 49;
    pub const GET_BOOTCHECKSUM: u8 = 177;
}

fn flash_oled(kb: &K86, fw: &[u8], mut progress: impl FnMut(f32)) -> Result<()> {
    use oled_cmd::*;
    if fw.len() <= SKIP {
        bail!("the screen firmware image is too small");
    }
    progress(0.0);
    let pre = Duration::from_millis(10);
    kb.raw_send(&[SET_BOOTLOADER, 85, 170, 85, 170, 0, 0, 0], Checksum::Bit7, pre)?;
    let mut ready = false;
    for _ in 0..5 {
        sleep(Duration::from_secs(3));
        kb.raw_send(&[GET_BOOTLOADER], Checksum::Bit7, pre)?;
        if kb.raw_read(Duration::from_millis(100))?[0] == GET_BOOTLOADER {
            ready = true;
            break;
        }
    }
    if !ready {
        bail!("the screen controller did not enter update mode");
    }
    let app = &fw[SKIP..];
    let chunks = app.len().div_ceil(64);
    kb.raw_send(&[SET_BOOTSTART, chunks as u8, (chunks >> 8) as u8], Checksum::Bit7, pre)?;
    for (i, c) in app.chunks(64).enumerate() {
        kb.raw_send(c, Checksum::None, pre)?;
        progress(i as f32 / chunks as f32);
    }
    let sum = byte_sum(app).to_le_bytes();
    kb.raw_send(&[GET_BOOTCHECKSUM, sum[0], sum[1], sum[2], sum[3]], Checksum::Bit7, pre)?;
    let r = kb.raw_read(Duration::from_secs(1))?;
    if r[1] != 85 {
        bail!("the screen controller rejected the update (checksum mismatch)");
    }
    progress(1.0);
    Ok(())
}

/// Watches the keyboard's input reports for a byte pattern (used to detect
/// "screen cleared"). Reads every hidraw node of the device in the background.
pub struct InputWatch {
    seen: Arc<Mutex<Vec<Vec<u8>>>>,
    stop: Arc<AtomicBool>,
}

impl InputWatch {
    pub fn open(vid: u16, pid: u16) -> Option<Self> {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let mut any = false;
        for info in hid::enumerate().into_iter().filter(|d| d.vid == vid && d.pid == pid) {
            let Ok(mut f) = OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK).open(&info.node) else {
                continue;
            };
            any = true;
            let (seen, stop) = (seen.clone(), stop.clone());
            std::thread::spawn(move || {
                let mut buf = [0u8; 128];
                while !stop.load(Ordering::Relaxed) {
                    match f.read(&mut buf) {
                        Ok(n) if n > 0 => seen.lock().unwrap().push(buf[..n].to_vec()),
                        _ => sleep(Duration::from_millis(20)),
                    }
                }
            });
        }
        any.then_some(Self { seen, stop })
    }

    /// Whether any report so far starts with `pat` (with or without a report id byte).
    pub fn saw(&self, pat: &[u8]) -> bool {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .any(|r| r.starts_with(pat) || r.get(1..).is_some_and(|s| s.starts_with(pat)))
    }
}

impl Drop for InputWatch {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_segments_compare_as_hex_digits() {
        assert_eq!(seg_version("V1.14"), Some(0x114));
        assert_eq!(seg_version("115"), Some(0x115));
        assert_eq!(seg_version("oledv106"), Some(0x106));
        assert_eq!(segments("114_oledv106"), vec!["114", "oledv106"]);
    }
}
